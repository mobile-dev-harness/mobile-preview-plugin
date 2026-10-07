import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = resolve(dirname(fileURLToPath(import.meta.url)), '../smoke-android-stream.py');
const python = process.env.PYTHON ?? 'python3';
const bootstrap = `import importlib.util, io, json, struct
spec = importlib.util.spec_from_file_location('smoke', ${JSON.stringify(script)})
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)
`;

function runPython(source) {
  const result = spawnSync(python, ['-B', '-c', bootstrap + source], { encoding: 'utf8', timeout: 10_000 });
  assert.equal(result.status, 0, result.stderr);
}

test('MPP1 parser rejects reserved bytes, stale generations, empty and oversized payloads', () => {
  runPython(`
valid = struct.pack('>4sB3sQIQ', b'MPP1', 1, b'\\0\\0\\0', 7, 4, 123)
assert smoke.parse_header(valid, 7) == (1, 4, 123)
for field, replacement in [(0, b'NOPE'), (1, 3), (2, b'\\0\\0x'), (3, 0), (3, 8), (4, 0), (4, 8388609)]:
    values = [b'MPP1', 1, b'\\0\\0\\0', 7, 4, 123]
    values[field] = replacement
    try:
        smoke.parse_header(struct.pack('>4sB3sQIQ', *values), 7)
    except smoke.SmokeError:
        pass
    else:
        raise AssertionError('invalid header accepted')
`);
});

test('configuration dimensions remain bounded and require codec bytes', () => {
  runPython(`
assert smoke.parse_configuration(struct.pack('>II', 576, 1280) + b'x') == {'width': 576, 'height': 1280}
for body in [b'', struct.pack('>II', 576, 1280), struct.pack('>II', 0, 1280) + b'x', struct.pack('>II', 1, 16385) + b'x']:
    try:
        smoke.parse_configuration(body)
    except smoke.SmokeError:
        pass
    else:
        raise AssertionError('invalid configuration accepted')
`);
});

test('JSON-lines reader drains already buffered replies without select readiness and bounds bad lines', () => {
  runPython(`
reader = smoke.JsonLines(io.BytesIO(b'{"id":1}\\n{"id":2}\\n'))
reader.thread.join(timeout=1)
assert reader.receive(.2) == {'id': 1}
assert reader.receive(.2) == {'id': 2}
for payload in [b'[]\\n', b'invalid\\n', b'x' * (smoke.MAX_LINE + 1)]:
    reader = smoke.JsonLines(io.BytesIO(payload))
    try:
        reader.receive(.2)
    except smoke.SmokeError:
        pass
    else:
        raise AssertionError('invalid JSON line accepted')
`);
});

test('a complete trailing packet header without payload cannot pass after valid configuration and key frame', () => {
  runPython(`
import socket,time
reader,writer = socket.socketpair()
video = smoke.Video(reader, 7, {'width': 576, 'height': 1280}, packet_timeout=.1)
def packet(kind,body):
    return struct.pack('>4sB3xQIQ',b'MPP1',kind,7,len(body),0)+body
try:
    writer.sendall(packet(0,struct.pack('>II',576,1280)+b'x')+packet(1,b'key'))
    deadline=time.monotonic()+1
    while video.metrics['packets']['key'] != 1 and time.monotonic()<deadline: time.sleep(.001)
    assert video.metrics['packets']['key'] == 1
    writer.sendall(struct.pack('>4sB3xQIQ',b'MPP1',2,7,4,0))
    while video.packet_deadline is None and time.monotonic()<deadline: time.sleep(.001)
    assert video.packet_deadline is not None
    # Simulate duration cutoff immediately after the incomplete packet began.
    started=time.monotonic()
    try:
        video.finish_packet()
    except smoke.SmokeError as error:
        assert 'Partial video packet timeout' in str(error)
    else:
        raise AssertionError('trailing payload was never sent')
    assert time.monotonic()-started < .5
finally:
    video.close();writer.close()
`);
});

test('packet deadline begins with first header byte and remains shared through payload', () => {
  runPython(`
import socket,time
reader,writer = socket.socketpair()
video = smoke.Video(reader,7,{'width':576,'height':1280},packet_timeout=.15)
header=struct.pack('>4sB3xQIQ',b'MPP1',0,7,12,0)
try:
    writer.sendall(header[:5])
    deadline=time.monotonic()+1
    while video.packet_deadline is None and time.monotonic()<deadline: time.sleep(.001)
    original=video.packet_deadline
    assert original is not None
    writer.sendall(header[5:]+b'x')
    try:
        video.finish_packet()
    except smoke.SmokeError as error:
        assert 'Partial video packet timeout' in str(error)
    else:
        raise AssertionError('truncated payload passed')
    assert video.packet_deadline == original
finally:
    video.close();writer.close()
`);
});

test('a duration cutoff on an idle screen does not wait for another video packet', () => {
  runPython(`
import socket,time
reader,writer=socket.socketpair()
video=smoke.Video(reader,7,{'width':576,'height':1280},packet_timeout=.15)
try:
    started=time.monotonic()
    video.finish_packet()
    assert time.monotonic()-started < .1
finally:
    video.close();writer.close()
`);
});

test('Home effect checks the resolved launcher on resumed lines, not arbitrary activity history', () => {
  runPython(`
package = smoke.launcher_package('priority=0\\ncom.android.launcher3/.Launcher')
assert package == 'com.android.launcher3'
assert smoke.launcher_resumed('mResumedActivity: ActivityRecord{a u0 com.android.launcher3/.Launcher t1}', package)
assert smoke.launcher_resumed('topResumedActivity=ActivityRecord{a u0 com.android.launcher3/.Launcher t1}', package)
assert not smoke.launcher_resumed('mResumedActivity: ActivityRecord{a u0 com.android.settings/.Settings t1}\\nhistory: com.android.launcher3/.Launcher', package)
assert not smoke.launcher_resumed('topResumedActivity=ActivityRecord{a u0 com.android.launcher30/.Launcher t1}', package)
try:
    smoke.launcher_package('android/com.android.internal.app.ResolverActivity')
except smoke.SmokeError:
    pass
else:
    raise AssertionError('chooser is not a launcher')
`);
});

test('invalid target and path arguments fail before ADB or host launch', () => {
  runPython(`
import argparse
def forbidden(*args):
    raise AssertionError('No device operation allowed during validation')
smoke.discover_adb = forbidden
for changed in [{'serial': ''}, {'serial': '-d'}, {'executable': 'relative/mpp'}, {'duration': float('nan')}, {'duration': 0}]:
    values = {'serial': 'explicit', 'executable': '/not/run', 'device_assets': '/not/read',
              'output': '/not/written', 'duration': 5, 'no_input': False, 'adb': None}
    values.update(changed)
    result = smoke.run(argparse.Namespace(**values))
    assert result['passed'] is False
    assert result['errors'][0]['stage'] == 'validate'
    assert result['cleanup'] == {}
`);
});

function fixture(t) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'mpp smoke ')));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const pythonPath = spawnSync(python, ['-c', 'import sys;print(sys.executable)'], { encoding: 'utf8' }).stdout.trim();
  assert(pythonPath.startsWith('/'));
  const executable = join(root, 'fake host');
  const adb = join(root, 'fake adb');
  const assets = join(root, 'device assets');
  const output = join(root, 'evidence', 'result.json');
  mkdirSync(assets);
  for (const filename of ['bootstrap.jar', 'libmpp_android_device.so']) writeFileSync(join(assets, filename), 'fixture');
  const trace = join(root, 'trace.jsonl');
  writeFileSync(adb, `#!${pythonPath}
import json,os,sys
args=sys.argv[1:]
assert args[:2] == ['-s', 'exact-fixture-serial']
with open(${JSON.stringify(trace)}, 'a') as f: f.write(json.dumps(args)+'\\n')
args=args[2:]
if args == ['shell','getprop','ro.build.version.sdk']: print('29')
elif args == ['shell','getprop','ro.product.cpu.abi']: print('arm64-v8a')
elif args == ['shell','getconf','PAGESIZE']: print('4096')
elif args == ['shell','cmd','package','resolve-activity','--brief','-a','android.intent.action.MAIN','-c','android.intent.category.HOME']: print('com.android.launcher3/.Launcher')
elif args == ['shell','dumpsys','activity','activities']:
    with open(${JSON.stringify(trace)}) as f: calls=[json.loads(line) for line in f]
    went_home=any(isinstance(call,dict) and call.get('control',{}).get('event',{}).get('phase') == 'up' for call in calls)
    component='com.android.launcher3/.Launcher' if went_home or os.environ.get('MPP_SMOKE_FIXTURE_AT_HOME') else 'com.android.settings/.Settings'
    print('topResumedActivity=ActivityRecord{a u0 '+component+' t1}')
elif args == ['reverse','--list']: pass
elif args == ['shell','ps','-A','-o','ARGS']: print('ARGS')
else: sys.exit(2)
`);
  writeFileSync(executable, `#!${pythonPath}
import json,os,socket,struct,sys,threading
servers=[]; peers=[]
def controls(server):
    peer,_=server.accept(); peers.append(peer)
    for line in peer.makefile('rb'):
        request=json.loads(line)
        with open(${JSON.stringify(trace)}, 'a') as f: f.write(json.dumps({'control':request['command']})+'\\n')
        peer.sendall((json.dumps({'seq':request['seq'],'ok':True})+'\\n').encode())
def video(server):
    peer,_=server.accept(); peers.append(peer)
    width = 720 if os.environ.get('MPP_SMOKE_FIXTURE_DIMENSIONS') else 576
    for kind,body in [(0,struct.pack('>II',width,1280)+b'configuration'),(1,b'keyframe')]:
        peer.sendall(struct.pack('>4sB3xQIQ',b'MPP1',kind,1,len(body),0)+body)
def reply(request,result):
    print(json.dumps({'schema':'mpp/v1','id':request['id'],'ok':True,'result':result}),flush=True)
for line in sys.stdin:
    request=json.loads(line); method=request['method']; params=request['params']
    with open(${JSON.stringify(trace)}, 'a') as f: f.write(json.dumps({'method':method})+'\\n')
    if method == 'devices.list': reply(request,{'devices':[{'id':'fixture','serial':'exact-fixture-serial','state':'online','kind':'emulator'}]})
    elif method == 'session.connect': reply(request,{'id':'lease','owner':params['owner'],'generation':1})
    elif method == 'preview.start':
        if os.environ.get('MPP_SMOKE_FIXTURE_FAIL'):
            print(json.dumps({'schema':'mpp/v1','id':request['id'],'ok':False,'error':{'code':'UNSUPPORTED','message':'secret-token-should-not-leak','hint':'secret-token-should-not-leak'}}),flush=True);continue
        paths={}
        for name,target in [('control',controls),('video',video)]:
            path=params['socket_dir']+'/'+name; server=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);server.bind(path);server.listen(1);servers.append(server);paths[name+'_socket']=path
            threading.Thread(target=target,args=(server,),daemon=True).start()
        reply(request,{**paths,'epoch':1,'generation':1,'stream_id':params['stream_id'],'geometry':{'width':576,'height':1280,'display_width':1080,'display_height':2400,'rotation':0}})
    elif method == 'preview.stop': reply(request,{'stopped':True})
    elif method == 'session.disconnect': reply(request,{'state':'disconnected'})
    else: sys.exit(3)
`);
  chmodSync(adb, 0o755);
  chmodSync(executable, 0o755);
  return { root, trace, output, run(extra = [], env = {}) {
    return spawnSync(python, ['-B', script, '--serial', 'exact-fixture-serial', '--executable', executable,
      '--device-assets', assets, '--output', output, '--adb', adb, '--duration', '0.15', ...extra],
    { encoding: 'utf8', timeout: 15_000, env: { ...process.env, ...env } });
  }, evidence() { return JSON.parse(readFileSync(output, 'utf8')); },
  calls() { return readFileSync(trace, 'utf8').trim().split('\n').map(JSON.parse); } };
}

test('fake host smoke covers exact serial, MPP Home control, video metadata and ordered cleanup', t => {
  const f = fixture(t);
  const result = f.run();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const evidence = f.evidence();
  assert.equal(evidence.passed, true);
  assert.deepEqual(evidence.device, { api: 29, abi: 'arm64-v8a', pageSize: 4096, kind: 'emulator' });
  assert.equal(evidence.homeStartedOutsideLauncher, true);
  assert.equal(evidence.homeEffectVerified, true);
  assert.equal(evidence.video.packets.configuration, 1);
  assert.equal(evidence.video.packets.key, 1);
  assert.deepEqual(evidence.control.map(row => row.command), ['key_frame', 'home_down', 'home_up']);
  assert.deepEqual(evidence.cleanup, { previewStopped: true, sessionDisconnected: true,
    host: { exitCode: 0, forced: false }, ownedReverseRemoved: true, ownedDeviceProcessGone: true });
  assert.deepEqual(f.calls().filter(row => row.method).map(row => row.method),
    ['devices.list', 'session.connect', 'preview.start', 'preview.stop', 'session.disconnect']);
  assert(!readFileSync(f.output, 'utf8').includes('exact-fixture-serial'));
});

test('--no-input avoids Home and activity inspection while retaining stream validation', t => {
  const f = fixture(t);
  const result = f.run(['--no-input']);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.equal(f.evidence().homeEffectVerified, null);
  assert.equal(f.evidence().homeStartedOutsideLauncher, null);
  assert.deepEqual(f.evidence().control.map(row => row.command), ['key_frame']);
  assert(!f.calls().some(row => Array.isArray(row) && row.includes('dumpsys')));
});

test('Home verification rejects an already resumed launcher without injecting Home or launching another app', t => {
  const f = fixture(t);
  const result = f.run([], { MPP_SMOKE_FIXTURE_AT_HOME: '1' });
  assert.equal(result.status, 1);
  const evidence = f.evidence();
  assert.equal(evidence.passed, false);
  assert.equal(evidence.homeStartedOutsideLauncher, false);
  assert.equal(evidence.homeEffectVerified, false);
  assert.match(evidence.errors[0].message, /switch to another app or use --no-input/);
  assert.deepEqual(evidence.control.map(row => row.command), ['key_frame']);
  assert(!f.calls().some(row => row.control?.kind === 'input'));
  assert(!f.calls().some(row => Array.isArray(row) && row.includes('start')));
  assert.equal(evidence.cleanup.previewStopped, true);
  assert.equal(evidence.cleanup.sessionDisconnected, true);
  assert.deepEqual(evidence.cleanup.host, { exitCode: 0, forced: false });
});

test('failed startup still disconnects and exits, with structured errors that exclude raw host diagnostics', t => {
  const f = fixture(t);
  const result = f.run([], { MPP_SMOKE_FIXTURE_FAIL: '1' });
  assert.equal(result.status, 1);
  const evidence = f.evidence();
  assert.equal(evidence.passed, false);
  assert.equal(evidence.cleanup.sessionDisconnected, true);
  assert.deepEqual(evidence.cleanup.host, { exitCode: 0, forced: false });
  assert.match(evidence.errors[0].message, /UNSUPPORTED/);
  assert(!JSON.stringify(evidence).includes('secret-token-should-not-leak'));
  assert(!result.stdout.includes('secret-token-should-not-leak'));
  assert(!result.stderr.includes('secret-token-should-not-leak'));
});

test('video dimensions inconsistent with the descriptor fail while all owned resources are closed', t => {
  const f = fixture(t);
  const result = f.run(['--no-input'], { MPP_SMOKE_FIXTURE_DIMENSIONS: '1' });
  assert.equal(result.status, 1);
  const evidence = f.evidence();
  assert.equal(evidence.passed, false);
  assert.match(evidence.errors[0].message, /dimensions differ/);
  assert.equal(evidence.cleanup.previewStopped, true);
  assert.equal(evidence.cleanup.sessionDisconnected, true);
  assert.deepEqual(evidence.cleanup.host, { exitCode: 0, forced: false });
});
