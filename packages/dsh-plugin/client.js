window.__ModuleLoader__.load({
  id: '@mobile-dev-harness/dsh-mobile-preview',
  factory(require) {
    const React = require('react');
    const { createSnapshotStore } = require('@deepseek-ai/dsh-client-store');
    const { Button, StateDot } = require('@deepseek-ai/dsh-client-ui-primitives');
    const h = React.createElement;
    const ID = '@mobile-dev-harness/dsh-mobile-preview';
    const KIND = 'mobile-preview';
    const NS = 'mobilePreview';
    const en = {
      title: 'Mobile devices', open: 'Connect a mobile device', guide: 'Choose a platform and connect a device',
      platform: 'Choose a platform', platformHelp: 'Choose what you are building. Your agent can also open the matching platform for this chat.',
      androidHelp: 'Emulators and USB devices', iosHelp: 'Xcode Simulator on macOS · Touch and Home when supported',
      iosPending: 'iOS Simulator requires a macOS host', iosPendingHelp: 'Your iOS selection is saved for this chat. Connect to a macOS host with Xcode installed to preview a simulator.',
      agentPlatform: 'Selected by your agent', userPlatform: 'Selected for this chat', selecting: 'Updating platform…',
      host: 'Connected host', waitingHost: 'Host not connected', hostHelp: 'Devices belong to the machine running DSH.',
      refresh: 'Refresh', refreshing: 'Refreshing…', devices: 'Choose a device', empty: 'No devices found',
      emptyHelp: 'Connect an Android device with USB debugging enabled, or create an AVD in Android Studio, then refresh.', iosEmptyHelp: 'Create an iOS Simulator in Xcode on this macOS host, then refresh.',
      online: 'Online', offline: 'Offline', unauthorized: 'Authorization required', stopped: 'Stopped', unknown: 'Unknown',
      emulator: 'Emulator', physical: 'Physical device', simulator: 'Simulator', android: 'Android', ios: 'iOS',
      connect: 'Connect', disconnect: 'Disconnect', start: 'Start selected emulator', starting: 'Starting emulator…',
      connecting: 'Connecting…', disconnecting: 'Disconnecting…', choose: 'Select a device to connect.',
      bootConsent: 'Starting opens the selected AVD on the connected host.', simulatorStart: 'Start selected simulator', simulatorStarting: 'Starting simulator…', simulatorConsent: 'Starting boots the selected iOS Simulator on the connected macOS host.',
      offlineHelp: 'Reconnect the device and refresh before connecting.',
      unauthorizedHelp: 'Unlock the device and accept its USB debugging prompt, then refresh.',
      current: 'This chat’s connection', connected: 'Transport ready', disconnected: 'Disconnected',
      uncertain: 'Checking connection', retained: 'The connection stays with this chat when you switch chats or close this panel.',
      phase: 'Android preview', previewUnavailable: 'Preview requires the configured Android capture backend and a browser with H.264 WebCodecs support.', iosPhase: 'iOS Simulator preview', iosPreviewHelp: 'Requires macOS, Xcode Simulator and H.264 WebCodecs. Supported simulators allow touch, drag and Home; others provide read-only video.', iosCanvasHelp: 'Click or drag in the preview, or press Home below. Use the Simulator app for keyboard input.', readOnly: 'Read-only preview', readOnlyHelp: 'Read-only video. Use the Simulator app for touch and keyboard input.',
      diagnostics: 'Connection details', previewReady: 'Connecting to the device display…',
      previewPause: 'Pause', previewResume: 'Resume', previewRetry: 'Retry', previewPaused: 'Preview paused',
      previewStart: 'Start preview', previewStop: 'Stop preview', previewStarting: 'Starting preview…', previewBuffering: 'Waiting for a video key frame…', previewLive: 'Live preview', previewStopped: 'Preview stopped',
      previewUnsupported: 'This browser cannot decode the device’s H.264 video.', streamEnded: 'The video stream ended. Start preview again to reconnect.', mediaInvalid: 'Invalid or stale video stream.', inputFailed: 'Input could not be confirmed. Preview stopped to release pressed controls.', home: 'Home', back: 'Back', canvasHelp: 'Click or drag in the preview. Focus it to use arrows, Enter, Backspace, Tab or Space.',
      error: 'Connection issue', warnings: 'Device discovery notices', noSession: 'Open a chat before connecting a device.',
      expired: 'The host lease expired while this page was inactive. Refresh and connect again.',
      stale: 'The device connection changed or ended. Refresh and connect again.',
      unreachable: 'The host could not be reached. Connection status is unverified.',
      invalidResponse: 'The host returned an invalid response. Reload the plugin and retry.',
      requestFailed: 'The host request failed. Check the host connection and retry.',
      busy: 'Wait for the current device operation to finish.', selectedRequired: 'Choose an online device first.',
      clientClosed: 'The device plugin has been unloaded.', unsupported: 'This device state cannot be connected.',
    };
    const zh = {
      title: '移动设备', open: '连接移动设备', guide: '选择开发平台并连接设备',
      platform: '选择开发平台', platformHelp: '选择正在开发的平台；Agent 也可以为此聊天打开对应的平台。',
      androidHelp: '模拟器与 USB 真机', iosHelp: 'macOS Xcode 模拟器 · 支持时可触控和回到主页',
      iosPending: 'iOS 模拟器需要 macOS 主机', iosPendingHelp: '已为此聊天选择 iOS。请连接已安装 Xcode 的 macOS 主机来预览模拟器。',
      agentPlatform: 'Agent 已选择平台', userPlatform: '此聊天已选择的平台', selecting: '正在切换平台…',
      host: '连接主机', waitingHost: '尚未连接主机', hostHelp: '设备位于运行 DSH 的这台主机上。',
      refresh: '刷新', refreshing: '正在刷新…', devices: '选择设备', empty: '未发现设备',
      emptyHelp: '连接已开启 USB 调试的 Android 设备，或在 Android Studio 中创建 AVD，然后刷新。', iosEmptyHelp: '请在此 macOS 主机的 Xcode 中创建 iOS 模拟器，然后刷新。',
      online: '在线', offline: '离线', unauthorized: '需要授权', stopped: '未启动', unknown: '未知',
      emulator: '模拟器', physical: '真机', simulator: '模拟器', android: 'Android', ios: 'iOS',
      connect: '连接', disconnect: '断开连接', start: '启动所选模拟器', starting: '正在启动模拟器…',
      connecting: '正在连接…', disconnecting: '正在断开…', choose: '请选择要连接的设备。',
      bootConsent: '将在连接主机上启动你选择的 AVD。', simulatorStart: '启动所选模拟器', simulatorStarting: '正在启动模拟器…', simulatorConsent: '将在连接的 macOS 主机上启动你选择的 iOS 模拟器。',
      offlineHelp: '请重新连接设备，刷新后再连接。',
      unauthorizedHelp: '请解锁设备并确认 USB 调试授权，然后刷新。',
      current: '此聊天的连接', connected: '设备通道已就绪', disconnected: '已断开',
      uncertain: '正在核实连接', retained: '切换聊天或关闭面板后，连接仍归属此聊天。',
      phase: 'Android 实时预览', previewUnavailable: '预览需要配置 Android 采集后端，并使用支持 H.264 WebCodecs 的浏览器。', iosPhase: 'iOS 模拟器预览', iosPreviewHelp: '需要 macOS、Xcode 模拟器和 H.264 WebCodecs。支持的模拟器可点击、拖动和回到主页；其他模拟器提供只读画面。', iosCanvasHelp: '在预览中点击或拖动，或点击下方主页按钮。键盘输入请使用模拟器应用。', readOnly: '只读预览', readOnlyHelp: '只读画面，请在模拟器应用中进行触控和键盘输入。',
      diagnostics: '连接详情', previewReady: '正在连接设备画面…',
      previewPause: '暂停', previewResume: '继续', previewRetry: '重试', previewPaused: '预览已暂停',
      previewStart: '开始预览', previewStop: '停止预览', previewStarting: '正在启动预览…', previewBuffering: '正在等待视频关键帧…', previewLive: '实时预览', previewStopped: '预览已停止',
      previewUnsupported: '当前浏览器无法解码设备的 H.264 视频。', streamEnded: '视频流已结束，请重新开始预览。', mediaInvalid: '视频流无效或已过期。', inputFailed: '无法确认输入已提交，预览已停止并释放按键。', home: '主页', back: '返回', canvasHelp: '在预览中点击或拖动，聚焦后可使用方向键、回车、退格、Tab 和空格。',
      error: '连接问题', warnings: '设备发现提示', noSession: '请先打开一个聊天，再连接设备。',
      expired: '页面处于后台期间，主机连接已过期。请刷新后重新连接。',
      stale: '设备连接已改变或结束。请刷新后重新连接。',
      unreachable: '暂时无法联系主机，尚不能确认设备连接状态。',
      invalidResponse: '主机返回的数据无效，请重新加载插件后重试。',
      requestFailed: '主机请求失败，请检查主机连接后重试。',
      busy: '请等待当前设备操作完成。', selectedRequired: '请先选择一台在线设备。',
      clientClosed: '设备插件已卸载。', unsupported: '当前设备状态不支持连接。',
    };

    function localError(key) { return { key, code: null, message: null, hint: null }; }
    function errorInfo(error) {
      if (error && (error.key || typeof error.message === 'string')) return error;
      return localError('requestFailed');
    }

    const MEDIA_LIMIT = 8 * 1024 * 1024;
    const safePositive = value => Number.isSafeInteger(value) && value > 0;

    function createMediaParser(generation) {
      if (!safePositive(generation)) throw localError('mediaInvalid');
      const header = new Uint8Array(28);
      let used = 0, payload = null, written = 0, metadata = null, failed = false;
      return {
        async push(bytes, consume) {
          if (failed || !(bytes instanceof Uint8Array)) throw localError('mediaInvalid');
          try {
            let offset = 0;
            while (offset < bytes.length) {
              if (!payload) {
                const count = Math.min(28 - used, bytes.length - offset);
                header.set(bytes.subarray(offset, offset + count), used); used += count; offset += count;
                if (used < 28) break;
                const view = new DataView(header.buffer);
                const kind = header[4], length = view.getUint32(16), pts = view.getBigUint64(20);
                if (view.getUint32(0) !== 0x4d505031 || kind > 2 || header[5] || header[6] || header[7]
                  || view.getBigUint64(8) !== BigInt(generation) || length > MEDIA_LIMIT
                  || length < (kind === 0 ? 9 : 1) || pts > BigInt(Number.MAX_SAFE_INTEGER)) throw localError('mediaInvalid');
                metadata = { kind, timestamp: Number(pts) };
                payload = new Uint8Array(length); written = 0;
              }
              const count = Math.min(payload.length - written, bytes.length - offset);
              payload.set(bytes.subarray(offset, offset + count), written); written += count; offset += count;
              if (written === payload.length) {
                const packet = { ...metadata, bytes: payload };
                payload = null; used = 0; written = 0;
                await consume(packet);
              }
            }
          } catch (error) { failed = true; throw error; }
        },
        finish() { if (failed || used || payload) throw localError('mediaInvalid'); },
      };
    }

    function annexBNals(bytes) {
      const nals = [];
      let start = -1;
      for (let i = 0; i + 2 < bytes.length; i++) {
        const prefix = bytes[i] === 0 && bytes[i + 1] === 0
          ? bytes[i + 2] === 1 ? 3 : bytes[i + 2] === 0 && bytes[i + 3] === 1 ? 4 : 0 : 0;
        if (!prefix) continue;
        if (start >= 0) {
          if (i === start || nals.length >= 4095) throw localError('mediaInvalid');
          nals.push(bytes.subarray(start, i));
        }
        else if (bytes.subarray(0, i).some(value => value !== 0)) throw localError('mediaInvalid');
        start = i + prefix; i += prefix - 1;
      }
      if (start < 0 || start >= bytes.length) throw localError('mediaInvalid');
      nals.push(bytes.subarray(start));
      return nals;
    }

    function createVideoPlayer(canvas, geometry, { state, keyFrame, failed }) {
      if (typeof VideoDecoder !== 'function' || typeof EncodedVideoChunk !== 'function'
        || typeof VideoDecoder.isConfigSupported !== 'function') throw localError('previewUnsupported');
      const context = canvas.getContext('2d');
      if (!context) throw localError('previewUnsupported');
      canvas.width = geometry.width; canvas.height = geometry.height;
      let decoder = null, config = null, codecBytes = null, needsKey = true, requestingKey = false;
      let stopped = false, epoch = 0, frame = null, raf = null, recoveries = 0;
      const capacityWaiters = new Set();
      function clear() {
        if (raf !== null) cancelAnimationFrame(raf);
        raf = null; frame?.close(); frame = null;
        context.clearRect(0, 0, canvas.width, canvas.height);
      }
      function closeDecoder() {
        const old = decoder; decoder = null;
        for (const cancel of [...capacityWaiters]) cancel();
        if (old && old.state !== 'closed') { try { old.close(); } catch { /* Already failed or closed. */ } }
      }
      function waitForCapacity(active, current) {
        if (stopped || active !== decoder || current !== epoch) return Promise.resolve(false);
        if (active.decodeQueueSize < 4) return Promise.resolve(true);
        if (typeof active.addEventListener !== 'function' || typeof active.removeEventListener !== 'function') {
          return Promise.reject(localError('previewUnsupported'));
        }
        return new Promise((resolve, reject) => {
          let settled = false;
          const finish = (error, ready = false) => {
            if (settled) return;
            settled = true; clearTimeout(timer);
            active.removeEventListener('dequeue', check);
            capacityWaiters.delete(cancel);
            if (error) reject(error); else resolve(ready);
          };
          const cancel = () => finish(null);
          const check = () => {
            if (stopped || active !== decoder || current !== epoch) finish(null);
            else if (active.state === 'closed') finish(localError('mediaInvalid'));
            else if (active.decodeQueueSize < 4) finish(null, true);
          };
          const timer = setTimeout(() => {
            // Window resizing may delay the event task even though the queue already drained.
            check();
            if (settled) return;
            finish(localError('mediaInvalid'));
          }, 2000);
          capacityWaiters.add(cancel);
          active.addEventListener('dequeue', check);
          // A dequeue between the initial check and listener registration is not lost.
          check();
        });
      }
      function install(current) {
        const active = new VideoDecoder({
          output(value) {
            if (stopped || current !== epoch || active !== decoder) { value.close(); return; }
            frame?.close(); frame = value;
            if (raf !== null) return;
            raf = requestAnimationFrame(() => {
              raf = null; const latest = frame; frame = null;
              if (!latest) return;
              try {
                if (!stopped && current === epoch && active === decoder) {
                  context.drawImage(latest, 0, 0, canvas.width, canvas.height); recoveries = 0; state('live');
                }
              } catch (error) { failed(error); }
              finally { latest.close(); }
            });
          },
          error(error) {
            if (!stopped && current === epoch && active === decoder) {
              try { recover(); } catch { failed(error); }
            }
          },
        });
        decoder = active; decoder.configure(config);
      }
      function recover() {
        if (stopped) return;
        if (++recoveries > 3) { stopped = true; closeDecoder(); clear(); failed(localError('mediaInvalid')); return; }
        needsKey = true; clear(); state('buffering');
        closeDecoder(); install(++epoch);
        if (!requestingKey) { requestingKey = true; keyFrame(); }
      }
      return {
        async consume(packet) {
          if (stopped) return;
          if (packet.kind === 0) {
            const view = new DataView(packet.bytes.buffer, packet.bytes.byteOffset, packet.bytes.byteLength);
            if (view.getUint32(0) !== geometry.width || view.getUint32(4) !== geometry.height) throw localError('mediaInvalid');
            const bytes = packet.bytes.subarray(8), nals = annexBNals(bytes);
            const sps = nals.find(nal => (nal[0] & 31) === 7);
            if (!sps || sps.length < 4 || !nals.some(nal => (nal[0] & 31) === 8)) throw localError('mediaInvalid');
            // Encoders may repeat SPS/PPS before every IDR; keep pending decoded frames alive.
            if (decoder?.state === 'configured' && codecBytes?.length === bytes.length
              && bytes.every((value, index) => value === codecBytes[index])) return;
            const current = ++epoch;
            closeDecoder(); clear(); needsKey = true; state('buffering');
            const codec = 'avc1.' + [...sps.subarray(1, 4)].map(value => value.toString(16).padStart(2, '0')).join('');
            const candidate = { codec, codedWidth: geometry.width, codedHeight: geometry.height, optimizeForLatency: true };
            let supported;
            try { supported = await VideoDecoder.isConfigSupported(candidate); }
            catch { throw localError('previewUnsupported'); }
            if (stopped || current !== epoch) return;
            if (!supported.supported) throw localError('previewUnsupported');
            config = candidate; codecBytes = bytes.slice(); requestingKey = false;
            install(current); return;
          }
          if (!decoder || !codecBytes) throw localError('mediaInvalid');
          const idr = annexBNals(packet.bytes).some(nal => (nal[0] & 31) === 5);
          if (packet.kind === 1 && !idr) throw localError('mediaInvalid');
          if (needsKey && !(packet.kind === 1 && idr)) return;
          const active = decoder, current = epoch;
          if (!await waitForCapacity(active, current)) return;
          if (stopped || active !== decoder || current !== epoch) return;
          const isKey = packet.kind === 1 && idr;
          let data = packet.bytes;
          if (isKey) {
            if (codecBytes.length + data.length > MEDIA_LIMIT) throw localError('mediaInvalid');
            const joined = new Uint8Array(codecBytes.length + data.length);
            joined.set(codecBytes); joined.set(data, codecBytes.length); data = joined;
            needsKey = false; requestingKey = false;
          }
          active.decode(new EncodedVideoChunk({ type: isKey ? 'key' : 'delta', timestamp: packet.timestamp, data }));
        },
        close() { if (stopped) return; stopped = true; epoch++; closeDecoder(); codecBytes = null; clear(); },
      };
    }

    function createInputQueue(epoch, send, onError) {
      if (!safePositive(epoch)) throw localError('mediaInvalid');
      let sequence = 0, commands = [], inFlight = null, scheduled = false, closed = false, closing = null;
      let pointer = false, keys = new Set(), heartbeat = null;
      const active = () => pointer || keys.size > 0;
      function held() {
        if (active() && heartbeat === null) heartbeat = setInterval(() => enqueue({ kind: 'heartbeat' }), 750);
        if (!active() && heartbeat !== null) { clearInterval(heartbeat); heartbeat = null; }
      }
      function requests(batch) {
        return batch.map(command => {
          if (sequence >= Number.MAX_SAFE_INTEGER) throw localError('inputFailed');
          return { seq: ++sequence, epoch, command };
        });
      }
      async function transmit(batch) {
        const outgoing = requests(batch), result = await send(outgoing);
        if (!Array.isArray(result?.replies) || result.replies.length !== outgoing.length
          || result.replies.some((reply, index) => reply.seq !== outgoing[index].seq || reply.ok !== true)) throw localError('inputFailed');
      }
      function stop(error) {
        if (closing) return closing;
        closed = true; commands = []; pointer = false; keys.clear(); held();
        closing = (async () => {
          await inFlight?.catch(() => {});
          try { await transmit([{ kind: 'reset' }]); } catch { /* Never replay uncertain input. */ }
          if (error) onError(errorInfo(error));
        })();
        return closing;
      }
      function flush() {
        scheduled = false;
        if (closed || inFlight || !commands.length) return;
        const batch = commands.splice(0, 64);
        const job = transmit(batch); inFlight = job;
        void job.then(() => { if (inFlight === job) inFlight = null; schedule(); }, error => {
          if (inFlight === job) inFlight = null;
          void stop(error);
        });
      }
      function schedule() { if (!closed && !scheduled) { scheduled = true; queueMicrotask(flush); } }
      function enqueue(command) {
        if (closed) return false;
        const previous = commands.at(-1);
        if (command.kind === 'input' && command.event.kind === 'touch' && command.event.phase === 'move'
          && previous?.kind === 'input' && previous.event.kind === 'touch' && previous.event.phase === 'move') commands[commands.length - 1] = command;
        else if (!(command.kind === 'heartbeat' && previous?.kind === 'heartbeat')) commands.push(command);
        if (commands.length > 128) { void stop(localError('inputFailed')); return false; }
        const event = command.event;
        if (command.kind === 'reset') { pointer = false; keys.clear(); }
        if (event?.kind === 'touch') { if (event.phase === 'down') pointer = true; if (event.phase === 'up' || event.phase === 'cancel') pointer = false; }
        if (event?.kind === 'key') { if (event.phase === 'down') keys.add(event.code); else keys.delete(event.code); }
        held(); schedule(); return true;
      }
      return { enqueue, reset: () => enqueue({ kind: 'reset' }), close: () => stop(),
        key(code) { enqueue({ kind: 'input', event: { kind: 'key', code, phase: 'down' } }); enqueue({ kind: 'input', event: { kind: 'key', code, phase: 'up' } }); } };
    }

    function bindCanvasInput(canvas, geometry, input, enabled, { keyboard = true } = {}) {
      let pointer = null, last = { x: 0, y: 0 };
      const pressed = new Set();
      const listeners = [];
      const listen = (target, event, fn) => { target.addEventListener(event, fn); listeners.push(() => target.removeEventListener(event, fn)); };
      function position(event) {
        const rect = canvas.getBoundingClientRect(), scale = Math.min(rect.width / geometry.width, rect.height / geometry.height);
        if (!scale || !Number.isFinite(scale)) return null;
        const width = geometry.width * scale, height = geometry.height * scale;
        const x = (event.clientX - rect.left - (rect.width - width) / 2) / width;
        const y = (event.clientY - rect.top - (rect.height - height) / 2) / height;
        return x >= 0 && x <= 1 && y >= 0 && y <= 1 ? { x, y } : null;
      }
      const touch = (phase, point) => input.enqueue({ kind: 'input', event: { kind: 'touch', phase, ...point, width: geometry.width, height: geometry.height } });
      function reset() {
        const captured = pointer; pointer = null; pressed.clear(); input.reset();
        if (captured !== null && canvas.hasPointerCapture?.(captured)) canvas.releasePointerCapture(captured);
      }
      listen(canvas, 'pointerdown', event => {
        if (!enabled() || pointer !== null || event.button !== 0 || event.isPrimary === false) return;
        const point = position(event); if (!point) return;
        event.preventDefault(); canvas.focus(); pointer = event.pointerId; last = point;
        canvas.setPointerCapture(pointer); touch('down', point);
      });
      listen(canvas, 'pointermove', event => {
        if (event.pointerId !== pointer) return;
        if (!enabled()) { reset(); return; }
        const point = position(event); if (!point) { reset(); return; }
        event.preventDefault(); last = point; touch('move', point);
      });
      listen(canvas, 'pointerup', event => {
        if (event.pointerId !== pointer) return;
        event.preventDefault(); const captured = pointer; pointer = null;
        const point = position(event); touch(point ? 'up' : 'cancel', point || last);
        if (canvas.hasPointerCapture?.(captured)) canvas.releasePointerCapture(captured);
      });
      for (const name of ['pointercancel', 'lostpointercapture', 'pointerleave']) listen(canvas, name, event => { if (event.pointerId === pointer) reset(); });
      if (keyboard) {
        const codes = { ArrowUp: 19, ArrowDown: 20, ArrowLeft: 21, ArrowRight: 22, Enter: 66, Backspace: 67, Tab: 61, ' ': 62 };
        listen(canvas, 'keydown', event => {
          const code = codes[event.key]; if (!enabled() || !code || event.altKey || event.ctrlKey || event.metaKey) return;
          event.preventDefault(); if (event.repeat || pressed.has(code)) return;
          pressed.add(code); input.enqueue({ kind: 'input', event: { kind: 'key', code, phase: 'down' } });
        });
        listen(canvas, 'keyup', event => {
          const code = codes[event.key]; if (!pressed.delete(code)) return;
          event.preventDefault(); input.enqueue({ kind: 'input', event: { kind: 'key', code, phase: 'up' } });
        });
      }
      listen(canvas, 'blur', reset); listen(window, 'blur', reset);
      const detach = () => { reset(); for (const remove of listeners) remove(); };
      detach.reset = reset;
      return detach;
    }

    function createController({ mountedSession, openPlatform }) {
      const source = createSnapshotStore({ host: null, devices: [], warnings: [], error: null, sessions: {} });
      const bindings = new Map();
      const previews = new Map();
      const previewIntents = new Map();
      const platformReads = new Map();
      const platformVersions = new Map();
      const handledIntents = new Map();
      let pageVisible = document.visibilityState !== 'hidden';
      const pending = new Set();
      let client = null;
      let opening = null;
      let generation = 0;
      let closed = false;
      let heartbeatPending = null;
      let statusPending = null;
      let timer = null;
      let lastHeartbeat = 0;
      let leaseTtl = 45000;
      let requestTimeout = 15000;
      let bootTimeout = 130000;
      let previewTimeout = 45000;
      const networkMargin = 2000;
      const patch = update => source.update(update);
      const row = (state, sessionId) => state.sessions[sessionId] ??= {
        binding: null, session: null, busy: null, error: null, notice: null, verified: false, preview: null, previewWanted: false,
        platform: null, platformSource: null, platformRevision: null, platformEpoch: null, platformAvailable: false, platformError: null,
      };

      function applyPlatform(sessionId, result) {
        if (result?.sessionId !== sessionId || ![null, 'android', 'ios'].includes(result.platform)
          || ![null, 'user', 'agent'].includes(result.source) || (result.platform === null) !== (result.source === null)
          || !Number.isSafeInteger(result.revision) || result.revision < 0
          || typeof result.epoch !== 'string' || !result.epoch || typeof result.available !== 'boolean') {
          throw localError('invalidResponse');
        }
        const previous = source.getSnapshot().sessions[sessionId];
        if (previous?.platformEpoch === result.epoch && previous.platformRevision > result.revision) return;
        patch(state => Object.assign(row(state, sessionId), {
          platform: result.platform, platformSource: result.source, platformRevision: result.revision,
          platformEpoch: result.epoch, platformAvailable: result.available, platformError: null,
        }));
      }

      function openAgentPlatform(sessionId) {
        if (closed || !pageVisible || document.visibilityState === 'hidden' || mountedSession() !== sessionId) return;
        const selection = source.getSnapshot().sessions[sessionId];
        if (selection?.platformSource !== 'agent') return;
        const key = `${selection.platformEpoch}:${selection.platformRevision}`;
        if (handledIntents.get(sessionId) === key) return;
        if (openPlatform(sessionId)) handledIntents.set(sessionId, key);
      }

      async function readPlatform(sessionId) {
        if (!sessionId || closed || source.getSnapshot().sessions[sessionId]?.busy === 'selecting') return;
        if (platformReads.has(sessionId)) return platformReads.get(sessionId);
        const version = (platformVersions.get(sessionId) || 0) + 1;
        platformVersions.set(sessionId, version);
        const job = call('platform.get', { sessionId }).then(result => {
          if (closed || platformVersions.get(sessionId) !== version) return;
          applyPlatform(sessionId, result);
          openAgentPlatform(sessionId);
        });
        platformReads.set(sessionId, job);
        try { await job; }
        finally { if (platformReads.get(sessionId) === job) platformReads.delete(sessionId); }
      }

      async function pollPlatform() {
        if (closed || !pageVisible || document.visibilityState === 'hidden') return;
        const sessionId = mountedSession();
        if (!sessionId) return;
        try { await readPlatform(sessionId); }
        catch (error) {
          if (!closed && mountedSession() === sessionId) {
            if (error?.code === 'CLIENT_EXPIRED') expire('expired');
            patch(state => { row(state, sessionId).platformError = errorInfo(error); });
          }
        }
      }
      const platformTimer = setInterval(() => { void pollPlatform(); }, 2000);
      queueMicrotask(() => { void pollPlatform(); });

      async function request(method, params, options = {}) {
        if (closed) throw localError('clientClosed');
        const abort = new AbortController();
        pending.add(abort);
        const cancel = () => abort.abort();
        options.signal?.addEventListener('abort', cancel, { once: true });
        if (options.signal?.aborted) abort.abort();
        const timer = setTimeout(() => abort.abort(),
          options.timeoutMs ?? ((['emulator.start', 'simulator.start'].includes(method) ? bootTimeout : method === 'preview.start' || method === 'preview.stop' ? previewTimeout : requestTimeout) + networkMargin));
        try {
          const response = await fetch('api/mobile-preview/v1', {
            method: 'POST', credentials: 'same-origin',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ method, params }), signal: abort.signal,
          });
          const body = await response.json();
          if (body?.ok === false && typeof body.error?.message === 'string') throw body.error;
          if (!response.ok || body?.ok !== true) throw localError('invalidResponse');
          return body.result;
        } catch (error) {
          if (error?.code || error?.key) throw error;
          throw localError('requestFailed');
        } finally {
          clearTimeout(timer);
          pending.delete(abort);
          options.signal?.removeEventListener('abort', cancel);
        }
      }

      function closeClient(token) {
        if (!token) return;
        void fetch('api/mobile-preview/v1', {
          method: 'POST', credentials: 'same-origin', keepalive: true,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ method: 'client.close', params: { client: token } }),
        }).catch(() => { /* Lease TTL bounds cleanup if the host is unreachable. */ });
      }

      function forget(sessionId, notice) {
        setWanted(sessionId, false);
        bindings.delete(sessionId);
        void reconcilePreview(sessionId);
        patch(state => Object.assign(row(state, sessionId), { binding: null, session: null, verified: false, notice, preview: null }));
      }

      function expire(notice) {
        const old = client;
        client = null;
        opening = null;
        generation += 1;
        clearInterval(timer);
        timer = null;
        for (const sessionId of bindings.keys()) forget(sessionId, notice);
        closeClient(old);
      }

      function failed(error, sessionId) {
        const info = errorInfo(error);
        if (info.code === 'CLIENT_EXPIRED') expire('expired');
        if (info.code === 'STALE_SESSION' || info.code === 'NOT_FOUND') {
          if (sessionId) forget(sessionId, 'stale');
        }
        patch(state => {
          if (sessionId) {
            row(state, sessionId).error = info;
            if (bindings.has(sessionId)) row(state, sessionId).verified = false;
          }
          else state.error = info;
        });
      }

      async function ensureClient() {
        if (closed) throw localError('clientClosed');
        if (client && Date.now() - lastHeartbeat >= leaseTtl) expire('expired');
        if (client) return client;
        if (!opening) {
          const openedAt = generation;
          const job = request('client.open', {}).then(result => {
            if (typeof result?.client !== 'string' || typeof result.host !== 'string'
              || !['leaseTtlMs', 'heartbeatMs', 'requestTimeoutMs', 'bootTimeoutMs', 'previewTimeoutMs'].every(
                key => Number.isSafeInteger(result[key]) && result[key] > 0)
              || result.heartbeatMs >= result.leaseTtlMs) {
              if (typeof result?.client === 'string') closeClient(result.client);
              throw localError('invalidResponse');
            }
            if (closed || generation !== openedAt) {
              closeClient(result.client);
              throw localError('clientClosed');
            }
            client = result.client;
            leaseTtl = result.leaseTtlMs;
            requestTimeout = result.requestTimeoutMs;
            bootTimeout = result.bootTimeoutMs;
            previewTimeout = result.previewTimeoutMs;
            lastHeartbeat = Date.now();
            clearInterval(timer);
            timer = setInterval(() => { void heartbeat(); }, result.heartbeatMs);
            patch(state => { state.host = result.host; state.error = null; });
            return client;
          });
          opening = job;
          void job.finally(() => { if (opening === job) opening = null; }).catch(_error => {
            // The caller receives the original opening rejection.
          });
        }
        return opening;
      }

      async function call(method, params, options) {
        const token = await ensureClient();
        const current = generation;
        const result = await request(method, { client: token, ...params }, options);
        if (closed || current !== generation || token !== client) throw localError('stale');
        return result;
      }

      function accept(sessionId, result) {
        if (!result) { forget(sessionId, null); return; }
        if (typeof result.binding !== 'string' || !result.session?.device) throw localError('invalidResponse');
        if (result.session.state === 'disconnected') { forget(sessionId, 'stale'); return; }
        const changed = bindings.get(sessionId) !== result.binding;
        bindings.set(sessionId, result.binding);
        if (changed) { setWanted(sessionId, true); clearPreviewFailure(sessionId); }
        patch(state => Object.assign(row(state, sessionId), {
          binding: result.binding, session: result.session, verified: true, notice: null, error: null,
          ...(changed ? { preview: null } : {}),
        }));
        if (changed) void reconcilePreview(sessionId);
      }

      async function inventory(platform) {
        const result = await call('devices.list', { platform });
        if (!Array.isArray(result?.devices) || !Array.isArray(result.warnings)) throw localError('invalidResponse');
        patch(state => {
          state.devices = [...state.devices.filter(device => device.platform !== platform),
            ...result.devices.filter(device => device.platform === platform)];
          state.warnings = result.warnings; state.error = null;
        });
      }

      async function run(sessionId, operation, work) {
        if (!sessionId) { failed(localError('noSession')); return; }
        if (source.getSnapshot().sessions[sessionId]?.busy) return;
        patch(state => Object.assign(row(state, sessionId), { busy: operation, error: null }));
        try { return await work(); }
        catch (error) { if (!closed) failed(error, sessionId); }
        finally { if (!closed) patch(state => { row(state, sessionId).busy = null; }); }
      }

      const activate = sessionId => run(sessionId, 'refreshing', async () => {
        await readPlatform(sessionId);
        const result = await call('session.list', { sessionId });
        if (result) accept(sessionId, result);
        else if (bindings.has(sessionId)) forget(sessionId, 'stale');
        const selection = source.getSnapshot().sessions[sessionId];
        const platform = selection?.platform || result?.session?.device?.platform;
        if (platform && (selection?.platformAvailable || result?.session?.device?.platform === platform)) await inventory(platform);
      });

      async function revalidate(token, current) {
        if (statusPending?.generation === current) return;
        const job = { generation: current };
        statusPending = job;
        try {
          await Promise.all([...bindings].map(async ([sessionId, binding]) => {
            try {
              const result = await request('session.status', { client: token, binding });
              if (!closed && current === generation && bindings.get(sessionId) === binding) accept(sessionId, result);
            } catch (error) {
              if (!closed && current === generation && bindings.get(sessionId) === binding) failed(error, sessionId);
            }
          }));
        } finally { if (statusPending === job) statusPending = null; }
      }

      async function heartbeat() {
        if (closed || !client) return;
        if (Date.now() - lastHeartbeat >= leaseTtl) { expire('expired'); return; }
        if (heartbeatPending?.generation === generation) return;
        const token = client;
        const current = generation;
        const job = { generation: current };
        heartbeatPending = job;
        try {
          await request('client.heartbeat', { client: token });
          if (closed || current !== generation || token !== client) return;
          lastHeartbeat = Date.now();
          patch(state => { state.error = null; });
          // Rust device work may queue behind a long boot; renewing the client must remain independent.
          void revalidate(token, current);
        } catch (error) {
          if (!closed && current === generation) {
            failed(error);
            patch(state => {
              for (const sessionId of bindings.keys()) Object.assign(row(state, sessionId), { verified: false, notice: 'unreachable' });
            });
          }
        } finally { if (heartbeatPending === job) heartbeatPending = null; }
      }


      function intentFor(sessionId) {
        if (!previewIntents.has(sessionId)) previewIntents.set(sessionId, {
          wanted: false, view: null, blocked: false, error: null, dirty: false, work: null,
        });
        return previewIntents.get(sessionId);
      }

      function setWanted(sessionId, wanted) {
        intentFor(sessionId).wanted = wanted;
        if (!closed) patch(state => { row(state, sessionId).previewWanted = wanted; });
      }

      function clearPreviewFailure(sessionId) {
        const intent = intentFor(sessionId);
        intent.blocked = false; intent.error = null;
        if (!closed && source.getSnapshot().sessions[sessionId]?.preview?.status === 'error') {
          patch(state => { row(state, sessionId).preview = { status: 'stopped', error: null, geometry: null }; });
        }
      }

      function desiredPreview(sessionId) {
        const intent = intentFor(sessionId), view = intent.view, binding = bindings.get(sessionId);
        return !closed && pageVisible && document.visibilityState !== 'hidden' && intent.wanted && !intent.blocked
          && binding && view && !view.disposed && !view.signal?.aborted ? { binding, view } : null;
      }

      function runtimeCurrent(sessionId, runtime) {
        const desired = desiredPreview(sessionId);
        return previews.get(sessionId) === runtime && !runtime.quiesced && desired?.view === runtime.view
          && desired.binding === runtime.binding && runtime.clientGeneration === generation && runtime.client === client;
      }

      function previewState(sessionId, runtime, status, error = null) {
        if (!runtimeCurrent(sessionId, runtime)) return;
        runtime.status = status;
        patch(state => { row(state, sessionId).preview = { status, error, geometry: runtime.descriptor?.geometry || null, capabilities: runtime.descriptor?.capabilities || null }; });
      }

      function quiescePreview(runtime) {
        if (runtime.quiesced) return;
        runtime.quiesced = true;
        clearTimeout(runtime.frameDeadline);
        runtime.player?.close(); runtime.detach?.();
        runtime.inputClosing = runtime.input?.close();
        if (!closed && previews.get(runtime.sessionId) === runtime) {
          const intent = intentFor(runtime.sessionId);
          patch(state => {
            const item = row(state, runtime.sessionId);
            if (item.binding === runtime.binding) item.preview = {
              status: intent.error ? 'error' : intent.wanted ? 'starting' : 'stopped',
              error: intent.error, geometry: item.preview?.geometry || runtime.descriptor?.geometry || null,
              capabilities: runtime.descriptor?.capabilities || item.preview?.capabilities || null,
            };
          });
        }
      }

      function failRuntime(sessionId, runtime, error) {
        if (!runtimeCurrent(sessionId, runtime)) return;
        const intent = intentFor(sessionId);
        intent.blocked = true; intent.error = errorInfo(error);
        quiescePreview(runtime);
        patch(state => { const item = row(state, sessionId); item.preview = { status: 'error', error: intent.error,
          geometry: item.preview?.geometry || runtime.descriptor?.geometry || null,
          capabilities: runtime.descriptor?.capabilities || item.preview?.capabilities || null }; });
        void reconcilePreview(sessionId);
      }

      async function disposePreview(sessionId, runtime) {
        quiescePreview(runtime);
        await runtime.inputClosing;
        runtime.abort.abort();
        try { await runtime.reader?.cancel(); } catch { /* The aborted stream may already be closed. */ }
        if (runtime.descriptor && runtime.client && !closed) {
          try { await request('preview.stop', { client: runtime.client, binding: runtime.binding, stream: runtime.descriptor.stream }); }
          catch (error) {
            if (bindings.get(sessionId) === runtime.binding && runtime.clientGeneration === generation
              && !['STALE_SESSION', 'NOT_FOUND', 'CLIENT_EXPIRED', 'CLOSED', 'HOST_EXITED'].includes(error?.code)) {
              const intent = intentFor(sessionId);
              intent.blocked = true; intent.error ||= errorInfo(error);
            }
          }
        }
        if (previews.get(sessionId) === runtime) previews.delete(sessionId);
        if (!closed) {
          const intent = intentFor(sessionId);
          patch(state => {
            const item = row(state, sessionId);
            if (item.binding === runtime.binding) item.preview = {
              status: intent.error ? 'error' : 'stopped', error: intent.error,
              geometry: item.preview?.geometry || runtime.descriptor?.geometry || null,
              capabilities: runtime.descriptor?.capabilities || item.preview?.capabilities || null,
            };
          });
        }
      }

      async function launchPreview(sessionId, desired) {
        const { binding, view } = desired;
        const runtime = { sessionId, binding, view, client, clientGeneration: generation, abort: new AbortController(),
          status: 'starting', descriptor: null, player: null, input: null, quiesced: false };
        previews.set(sessionId, runtime); previewState(sessionId, runtime, 'starting');
        const current = () => runtimeCurrent(sessionId, runtime);
        try {
          const token = await ensureClient();
          if (!current() || token !== runtime.client) return;
          // Await even an obsolete start receipt, then stop that exact stream before another start.
          // Aborting only the HTTP request can hide a backend stream that is still starting.
          runtime.descriptor = await request('preview.start', { client: runtime.client, binding });
          const descriptor = runtime.descriptor;
          if (!current()) return;
          const canvas = view.canvas;
          const geometry = descriptor?.geometry;
          if (typeof descriptor?.stream !== 'string' || !safePositive(descriptor.epoch) || !safePositive(descriptor.generation)
            || descriptor.generation !== source.getSnapshot().sessions[sessionId]?.session?.generation
            || !geometry || !['width', 'height', 'display_width', 'display_height'].every(key => safePositive(geometry[key]) && geometry[key] <= 16384)
            || geometry.width > 4096 || geometry.height > 4096 || !Number.isInteger(geometry.rotation) || geometry.rotation < 0 || geometry.rotation > 3
            || descriptor.capabilities?.video !== true || typeof descriptor.capabilities.input !== 'boolean') throw localError('invalidResponse');
          const failPreview = error => { if (current()) failRuntime(sessionId, runtime, error?.key || error?.code ? error : localError('mediaInvalid')); };
          runtime.input = createInputQueue(descriptor.epoch,
            requests => request('input.send', { client: runtime.client, binding, stream: descriptor.stream, requests }, { timeoutMs: 1500 }),
            () => failPreview(localError('inputFailed')));
          runtime.player = createVideoPlayer(canvas, geometry, {
            state(status) {
              if (!current()) return;
              if (status === 'live') clearTimeout(runtime.frameDeadline);
              else if (status === 'buffering' && runtime.status === 'live') {
                runtime.detach?.reset();
                clearTimeout(runtime.frameDeadline);
                runtime.frameDeadline = setTimeout(() => failPreview(localError('streamEnded')), 10000);
              }
              previewState(sessionId, runtime, status);
            },
            keyFrame: () => runtime.input.enqueue({ kind: 'key_frame' }), failed: failPreview,
          });
          const device = source.getSnapshot().sessions[sessionId]?.session?.device;
          if (descriptor.capabilities.input === true) {
            runtime.detach = bindCanvasInput(canvas, geometry, runtime.input,
              () => current() && runtime.status === 'live', { keyboard: device?.platform !== 'ios' });
          }
          previewState(sessionId, runtime, 'buffering');
          runtime.frameDeadline = setTimeout(() => failPreview(localError('streamEnded')), 10000);
          const parser = createMediaParser(descriptor.generation);
          const pump = async () => {
            const response = await fetch('api/mobile-preview/v1/media', {
              method: 'POST', credentials: 'same-origin', headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ client: runtime.client, binding, stream: descriptor.stream }), signal: runtime.abort.signal,
            });
            if (!current()) { await response.body?.cancel?.(); return; }
            if (!response.ok || !response.headers?.get('content-type')?.startsWith('application/octet-stream') || !response.body) {
              let error; try { error = (await response.json())?.error; } catch { /* Not a JSON response. */ }
              throw error || localError('mediaInvalid');
            }
            runtime.reader = response.body.getReader();
            while (current()) {
              const { done, value } = await runtime.reader.read();
              if (!current()) return;
              if (done) { parser.finish(); throw localError('streamEnded'); }
              await parser.push(value, packet => runtime.player.consume(packet));
            }
          };
          void pump().catch(error => { if (current()) failPreview(error); });
        } catch (error) {
          if (current()) failRuntime(sessionId, runtime, error);
        }
      }

      function reconcilePreview(sessionId) {
        const intent = intentFor(sessionId);
        intent.dirty = true;
        const runtime = previews.get(sessionId);
        if (runtime && !runtimeCurrent(sessionId, runtime)) quiescePreview(runtime);
        if (intent.work) return intent.work;
        const work = Promise.resolve().then(async () => {
          while (intent.dirty) {
            intent.dirty = false;
            const active = previews.get(sessionId);
            if (active && !runtimeCurrent(sessionId, active)) {
              await disposePreview(sessionId, active);
              intent.dirty = true;
              continue;
            }
            const desired = desiredPreview(sessionId);
            if (!active && desired) {
              await launchPreview(sessionId, desired);
              const launched = previews.get(sessionId);
              if (launched && !runtimeCurrent(sessionId, launched)) intent.dirty = true;
            }
          }
        }).catch(error => {
          intent.blocked = true; intent.error = errorInfo(error);
          const active = previews.get(sessionId);
          if (active) quiescePreview(active);
          if (!closed) patch(state => { row(state, sessionId).preview = { status: 'error', error: intent.error, geometry: null }; });
        }).finally(() => {
          if (intent.work === work) intent.work = null;
          if (intent.dirty) return reconcilePreview(sessionId);
        });
        intent.work = work;
        return work;
      }

      function mountPreview(sessionId, canvas, signal) {
        if (closed || !sessionId || !canvas || signal?.aborted) return () => {};
        const intent = intentFor(sessionId);
        if (intent.view?.canvas === canvas && intent.view.signal === signal && !intent.view.disposed) return intent.view.dispose;
        const previous = intent.view;
        const view = { canvas, signal, disposed: false, dispose: null };
        view.dispose = () => {
          if (view.disposed) return;
          view.disposed = true; signal?.removeEventListener('abort', view.dispose);
          if (intent.view === view) { intent.view = null; void reconcilePreview(sessionId); }
        };
        intent.view = view;
        previous?.dispose();
        signal?.addEventListener('abort', view.dispose, { once: true });
        clearPreviewFailure(sessionId);
        void reconcilePreview(sessionId);
        return view.dispose;
      }

      function pausePreview(sessionId) {
        setWanted(sessionId, false);
        return reconcilePreview(sessionId);
      }

      function resumePreview(sessionId) {
        if (closed) return Promise.resolve();
        setWanted(sessionId, true); clearPreviewFailure(sessionId);
        return reconcilePreview(sessionId);
      }

      function startPreview(sessionId, canvas) {
        mountPreview(sessionId, canvas);
        return resumePreview(sessionId);
      }

      function stopPreview(sessionId) {
        intentFor(sessionId).view?.dispose();
        return reconcilePreview(sessionId);
      }

      const stopAllPreviews = () => { for (const id of previewIntents.keys()) void stopPreview(id); };
      function visibility(visible) {
        if (visible !== pageVisible) {
          pageVisible = visible;
          for (const id of previewIntents.keys()) {
            if (visible) clearPreviewFailure(id);
            void reconcilePreview(id);
          }
        }
        if (visible) { void heartbeat(); void pollPlatform(); }
      }
      const wake = () => visibility(document.visibilityState !== 'hidden');
      const hidePage = () => visibility(false);
      document.addEventListener('visibilitychange', wake);
      window.addEventListener('pageshow', wake);
      window.addEventListener('pagehide', hidePage);
      return {
        source,
        activate, pollPlatform, startPreview, stopPreview, stopAllPreviews, mountPreview, pausePreview, resumePreview,
        selectPlatform: (sessionId, platform) => run(sessionId, 'selecting', async () => {
          const version = (platformVersions.get(sessionId) || 0) + 1;
          platformVersions.set(sessionId, version);
          const result = await call('platform.select', { sessionId, platform });
          if (platformVersions.get(sessionId) !== version) return;
          applyPlatform(sessionId, result);
          if (result.available) await inventory(result.platform);
        }),
        pressKey: (sessionId, code) => {
          const runtime = previews.get(sessionId);
          const ios = source.getSnapshot().sessions[sessionId]?.session?.device?.platform === 'ios';
          if (runtime && runtimeCurrent(sessionId, runtime) && runtime.status === 'live'
            && runtime.descriptor.capabilities.input === true && (!ios || code === 3)) runtime.input.key(code);
        },
        connect: (sessionId, device) => run(sessionId, 'connecting', async () => {
          accept(sessionId, await call('session.connect', { sessionId, device }));
        }),
        disconnect: sessionId => run(sessionId, 'disconnecting', async () => {
          await pausePreview(sessionId);
          const binding = bindings.get(sessionId);
          if (binding) await call('session.disconnect', { binding });
          forget(sessionId, 'disconnected');
        }),
        start: (sessionId, avd) => run(sessionId, 'starting', async () => {
          const device = await call('emulator.start', { avd, consent: true });
          await inventory('android');
          return device;
        }),
        startSimulator: (sessionId, udid) => run(sessionId, 'starting', async () => {
          const device = await call('simulator.start', { udid, consent: true });
          await inventory('ios');
          return device;
        }),
        close() {
          if (closed) return;
          for (const id of previewIntents.keys()) { setWanted(id, false); intentFor(id).view?.dispose(); }
          for (const runtime of previews.values()) { quiescePreview(runtime); runtime.abort.abort(); }
          closed = true;
          generation += 1;
          clearInterval(timer);
          clearInterval(platformTimer);
          document.removeEventListener('visibilitychange', wake);
          window.removeEventListener('pageshow', wake);
          window.removeEventListener('pagehide', hidePage);
          for (const abort of pending) abort.abort();
          closeClient(client);
          client = null;
          bindings.clear();
        },
      };
    }

    const styles = {
      panel: { display: 'flex', flexDirection: 'column', gap: 20, padding: 16, height: '100%', boxSizing: 'border-box', overflow: 'auto', color: 'var(--dsw-alias-label-primary)', background: 'var(--dsw-alias-bg-base)', fontSize: 13 },
      row: { display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 },
      stack: { display: 'flex', flexDirection: 'column', gap: 8 },
      muted: { margin: 0, color: 'var(--dsw-alias-label-secondary)', fontSize: 12, lineHeight: 1.55 },
      card: { padding: 12, border: '1px solid var(--dsw-alias-border-l2)', borderRadius: 'var(--dsw-radius-md)', background: 'var(--dsw-alias-bg-layer-1)' },
      heading: { margin: 0, fontSize: 13, fontWeight: 600 },
      detail: { overflowWrap: 'anywhere', fontFamily: 'monospace', fontSize: 11, color: 'var(--dsw-alias-label-secondary)' },
      error: { padding: 12, borderRadius: 'var(--dsw-radius-md)', color: 'var(--dsw-alias-state-error-primary)', background: 'var(--dsw-alias-bg-layer-1)', fontSize: 12, lineHeight: 1.5, overflowWrap: 'anywhere' },
    };

    function DeviceIcon() {
      return h('svg', { width: 18, height: 18, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 1.6, 'aria-hidden': true },
        h('rect', { x: 6, y: 2.5, width: 12, height: 19, rx: 2.5 }), h('path', { d: 'M10 5h4M10 18.5h4' }));
    }

    function EntryButton({ sessionId, open, t }) {
      return h(Button, { variant: 'toolbar', size: 'sm', disabled: !sessionId,
        title: t('open'), 'aria-label': t('open'), onClick: () => open(sessionId), icon: h(DeviceIcon) });
    }

    function ErrorMessage({ error, t }) {
      if (!error) return null;
      return h('div', { role: 'alert', style: styles.error },
        h('strong', null, t('error')), h('div', null, error.key ? t(error.key) : error.message),
        error.hint ? h('div', null, error.hint) : null,
        error.code ? h('code', { style: { fontSize: 11 } }, error.code) : null);
    }

    const DEVICE_CHROME = { rim: 2, horizontalPadding: 8, verticalPadding: 10, gutter: 4 };
    const deviceAspect = geometry => Number.isFinite(geometry?.width) && geometry.width > 0
      && Number.isFinite(geometry?.height) && geometry.height > 0 ? geometry.width / geometry.height : 9 / 20;
    const deviceHorizontalChrome = () => 2 * (DEVICE_CHROME.rim + DEVICE_CHROME.horizontalPadding);
    const deviceVerticalChrome = () => 2 * (DEVICE_CHROME.rim + DEVICE_CHROME.verticalPadding);

    function deviceFrameSize(bounds, geometry) {
      const width = bounds.width - DEVICE_CHROME.gutter * 2 - deviceHorizontalChrome();
      const height = bounds.height - DEVICE_CHROME.gutter * 2 - deviceVerticalChrome();
      if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return null;
      const aspect = deviceAspect(geometry), screenHeight = Math.min(height, width / aspect);
      return { width: screenHeight * aspect + deviceHorizontalChrome(), height: screenHeight + deviceVerticalChrome() };
    }

    function createWidthFit(requestWidth, initial) {
      const noop = { update() {}, dispose() {} };
      if (typeof requestWidth !== 'function' || typeof ResizeObserver !== 'function'
        || typeof requestAnimationFrame !== 'function' || typeof cancelAnimationFrame !== 'function'
        || typeof getComputedStyle !== 'function' || !initial.section || initial.signal?.aborted) return noop;
      let config = initial, handle = null, lastWidth = null, frame = null, disposed = false;
      const number = value => Number.parseFloat(value) || 0;
      const measure = () => {
        frame = null;
        if (disposed || !config.section) return;
        const style = getComputedStyle(config.section);
        const height = config.viewport ? config.viewport.getBoundingClientRect().height
          : config.section.getBoundingClientRect().height - number(style.paddingTop) - number(style.paddingBottom);
        const aspect = deviceAspect(config.geometry);
        const contentWidth = config.viewport
          ? Math.max(0, height - DEVICE_CHROME.gutter * 2 - deviceVerticalChrome()) * aspect
            + deviceHorizontalChrome() + DEVICE_CHROME.gutter * 2
          : height * aspect;
        const width = Math.round(contentWidth + number(style.paddingLeft) + number(style.paddingRight));
        if (!Number.isFinite(height) || height <= 0 || !Number.isFinite(width) || width <= 0 || width === lastWidth) return;
        lastWidth = width;
        // Keep this handle after a manual drag; the host deliberately ignores its later updates.
        if (handle) handle.update(width);
        else handle = requestWidth(width);
      };
      const schedule = () => {
        if (!disposed && frame === null) frame = requestAnimationFrame(measure);
      };
      const observer = new ResizeObserver(schedule);
      const observe = () => {
        observer.disconnect();
        if (config.section) observer.observe(config.section);
        if (config.viewport && config.viewport !== config.section) observer.observe(config.viewport);
      };
      const dispose = () => {
        if (disposed) return;
        disposed = true;
        observer.disconnect();
        if (frame !== null) cancelAnimationFrame(frame);
        frame = null;
        initial.signal?.removeEventListener('abort', dispose);
        handle?.dispose();
      };
      initial.signal?.addEventListener('abort', dispose, { once: true });
      observe(); schedule();
      return {
        update(next) {
          if (disposed) return;
          const previous = config;
          config = { ...config, ...next };
          if (config.section !== previous.section || config.viewport !== previous.viewport) observe();
          schedule();
        },
        dispose,
      };
    }

    function Panel({ sessionId, usePreview, useTabInfo, activate, selectPlatform, connect, disconnect, start, startSimulator, mountPreview, pausePreview, resumePreview, pressKey, beginWidthFit, t }) {
      const state = usePreview(value => value);
      const { tab, sidebar, panel } = useTabInfo();
      const session = state.sessions[sessionId];
      const [choice, setChoice] = React.useState(null);
      const platform = session?.platform;
      const selected = choice?.sessionId === sessionId && choice.platform === platform ? choice.id : '';
      const setSelected = id => setChoice({ sessionId, platform, id });
      const canvas = React.useRef(null);
      const section = React.useRef(null);
      const viewport = React.useRef(null);
      const widthFit = React.useRef(null);
      const [frameSize, setFrameSize] = React.useState(null);
      const preview = session?.preview;
      const previewActive = ['starting', 'buffering', 'live'].includes(preview?.status);
      const wanted = session?.previewWanted !== false;
      const live = wanted && preview?.status === 'live';
      const connected = Boolean(session?.binding);
      const fitEligible = tab.visible && !tab.signal.aborted && sidebar?.fullscreen === false && panel?.isSoleDockedPane === true;
      // Geometry and connection changes update the same claim so manual resizing keeps priority.
      React.useEffect(() => {
        if (!fitEligible || typeof beginWidthFit !== 'function') return;
        const fit = beginWidthFit({ section: section.current, viewport: viewport.current,
          geometry: preview?.geometry, signal: tab.signal });
        widthFit.current = fit;
        return () => { fit.dispose(); if (widthFit.current === fit) widthFit.current = null; };
      }, [sessionId, fitEligible, tab.signal, beginWidthFit]);
      React.useEffect(() => {
        widthFit.current?.update({ section: section.current, viewport: viewport.current, geometry: preview?.geometry });
      }, [connected, preview?.geometry?.width, preview?.geometry?.height]);
      React.useEffect(() => {
        const node = viewport.current;
        if (!connected || !node || typeof ResizeObserver !== 'function'
          || typeof requestAnimationFrame !== 'function' || typeof cancelAnimationFrame !== 'function') return;
        let frame = null, disposed = false;
        const measure = () => {
          frame = null;
          if (disposed) return;
          const next = deviceFrameSize(node.getBoundingClientRect(), preview?.geometry);
          setFrameSize(previous => previous?.width === next?.width && previous?.height === next?.height ? previous : next);
        };
        const schedule = () => { if (!disposed && frame === null) frame = requestAnimationFrame(measure); };
        const observer = new ResizeObserver(schedule);
        observer.observe(node); schedule();
        return () => { disposed = true; observer.disconnect(); if (frame !== null) cancelAnimationFrame(frame); };
      }, [connected, preview?.geometry?.width, preview?.geometry?.height]);
      React.useEffect(() => {
        if (tab.visible && !tab.signal.aborted) void activate();
      }, [sessionId, tab.visible, tab.signal, platform, activate]);
      React.useEffect(() => {
        if (!connected || !tab.visible || tab.signal.aborted || !canvas.current || typeof mountPreview !== 'function') return;
        return mountPreview(canvas.current, tab.signal);
      }, [sessionId, session?.binding, connected, tab.visible, tab.signal, mountPreview]);
      const devices = session?.platformAvailable ? state.devices.filter(item => item.platform === platform) : [];
      const device = devices.find(item => item.id === selected);
      const busy = Boolean(session?.busy);
      const selectedState = ['online', 'offline', 'unauthorized', 'stopped'].includes(device?.state) ? device.state : 'unknown';
      const error = session?.error || session?.platformError || state.error;
      const text = key => t(key);
      if (connected) {
        const previewStatus = !wanted ? 'previewPaused' : preview?.status === 'starting' ? 'previewStarting'
          : preview?.status === 'buffering' ? 'previewBuffering' : live ? 'previewLive' : 'previewStopped';
        const connection = session.session.device;
        const ios = connection.platform === 'ios';
        const interactive = preview?.capabilities?.input ?? (ios ? connection.capabilities?.input === true : true);
        const canvasHelp = interactive ? ios ? 'iosCanvasHelp' : 'canvasHelp' : 'readOnlyHelp';
        return h('section', { ref: section, style: { ...styles.panel, gap: 10, padding: 12, minHeight: 0, overflow: 'hidden' },
          'aria-label': text('title'), 'data-mobile-preview': '', 'data-mobile-preview-connected': '' },
          h('div', { style: { ...styles.stack, gap: 4, flexShrink: 0 } },
            h('div', { style: styles.row },
              h('h2', { style: { ...styles.heading, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }, title: connection.name }, connection.name),
              h(Button, { size: 'sm', variant: 'ghost', disabled: busy, onClick: () => { void disconnect(); } }, text(session.busy === 'disconnecting' ? 'disconnecting' : 'disconnect'))),
            h('div', { style: { ...styles.row, ...styles.muted, justifyContent: 'flex-start', gap: 6 } },
              h(StateDot, { state: session.verified ? 'done' : 'warning' }), text(session.verified ? 'connected' : 'uncertain'),
              h('span', { style: { marginLeft: 'auto', border: '1px solid var(--dsw-alias-border-l2)', borderRadius: 4, padding: '1px 5px' } }, text(connection.platform === 'ios' ? 'ios' : 'android'))),
            h('p', { style: { ...styles.muted, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }, title: state.host || '' }, `${text('host')}: ${state.host || text('waitingHost')}`)),
          error || preview?.error ? h('div', { style: { flexShrink: 0, maxHeight: 90, overflow: 'auto' } },
            h(ErrorMessage, { error: preview?.error || error, t })) : null,
          session.notice && session.notice !== 'disconnected'
            ? h('p', { role: 'status', style: { ...styles.muted, flexShrink: 0 } }, text(session.notice)) : null,
          h('div', { ref: viewport, 'data-mobile-preview-viewport': '', style: { flex: '1 1 0', minHeight: 0, minWidth: 0, position: 'relative', display: 'flex', alignItems: 'center', justifyContent: 'center' } },
            h('div', { 'data-mobile-preview-device': '', style: { boxSizing: 'border-box', position: 'relative', flexShrink: 0,
              width: frameSize?.width || 0, height: frameSize?.height || 0, visibility: frameSize ? 'visible' : 'hidden',
              padding: `${DEVICE_CHROME.verticalPadding}px ${DEVICE_CHROME.horizontalPadding}px`,
              border: `${DEVICE_CHROME.rim}px solid #5c6068`, borderRadius: 30, background: '#090b0e',
              boxShadow: 'inset 0 0 0 1px #1d2026, 0 0 0 1px #030405, 0 8px 20px #0005' } },
              h('div', { 'aria-hidden': true, style: { position: 'absolute', top: 4, left: '50%', transform: 'translateX(-50%)', width: 28, height: 3, borderRadius: 3, background: '#30343b', pointerEvents: 'none' } }),
              ...[{ side: 'left', top: '17%', height: 23 }, { side: 'left', top: '23%', height: 37 }, { side: 'right', top: '25%', height: 45 }].map((button, index) =>
                h('div', { key: index, 'aria-hidden': true, style: { position: 'absolute', [button.side]: -4, top: button.top, width: 3, height: button.height, borderRadius: 2, background: 'linear-gradient(90deg, #25282f, #5b6069)', pointerEvents: 'none' } })),
              h('div', { 'data-mobile-preview-screen': '', style: { position: 'relative', width: '100%', height: '100%', overflow: 'hidden', borderRadius: 18, background: '#000' } },
                h('canvas', { ref: canvas, tabIndex: interactive ? 0 : -1, 'aria-label': text(canvasHelp),
                  style: { position: 'absolute', inset: 0, width: '100%', height: '100%', objectFit: 'contain', display: 'block', touchAction: interactive ? 'none' : 'auto' } }),
                !live ? h('div', { style: { position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 20, color: '#c4c4c4', textAlign: 'center', fontSize: 12, lineHeight: 1.6, pointerEvents: 'none' } },
                  text(previewActive ? previewStatus : !wanted ? 'previewPaused' : preview?.error ? 'previewStopped' : 'previewReady')) : null))),
          h('div', { 'data-mobile-preview-controls': '', style: { ...styles.stack, flexShrink: 0, gap: 8 } },
            h('p', { role: 'status', style: { ...styles.muted, display: 'flex', alignItems: 'center', gap: 6 } },
              h(StateDot, { state: live ? 'done' : 'idle' }), text(previewStatus)),
            h('div', { style: styles.row },
              interactive ? h('div', { style: { ...styles.row, justifyContent: 'flex-start', gap: 6 } },
                h(Button, { size: 'sm', disabled: !live, onClick: () => pressKey(3) }, text('home')),
                !ios ? h(Button, { size: 'sm', disabled: !live, onClick: () => pressKey(4) }, text('back')) : null) : h('span', { style: styles.muted }, text('readOnly')),
              h(Button, { size: 'sm', variant: wanted && !preview?.error ? 'outline' : 'primary', disabled: busy,
                onClick: () => { void (wanted && !preview?.error ? pausePreview() : resumePreview()); } },
              text(preview?.error && wanted ? 'previewRetry' : wanted ? 'previewPause' : 'previewResume')))),
          h('details', { style: { ...styles.muted, flexShrink: 0, maxHeight: '20%', overflow: 'auto' } },
            h('summary', { style: { cursor: 'pointer' } }, text('diagnostics')),
            h('div', { style: { ...styles.stack, paddingTop: 8 } },
              h('code', { style: styles.detail }, connection.serial || connection.avd || connection.id),
              h('p', { style: styles.muted }, text('retained')),
              h('p', { style: styles.muted }, text(canvasHelp)),
              h('p', { style: styles.muted }, text('hostHelp')),
              state.warnings.length ? h('ul', { style: { margin: 0, paddingLeft: 18 } },
                state.warnings.map((warning, index) => h('li', { key: index }, warning))) : null,
              h(Button, { size: 'sm', variant: 'ghost', disabled: busy, onClick: () => { void activate(); } }, text(session.busy === 'refreshing' ? 'refreshing' : 'refresh')))));
      }
      return h('section', { ref: section, style: styles.panel, 'aria-label': text('title'), 'data-mobile-preview': '' },
        h('div', { style: styles.stack },
          h('div', { style: styles.row }, h('h2', { style: styles.heading }, text('title')),
            session?.platformAvailable ? h(Button, { size: 'sm', variant: 'ghost', disabled: busy, onClick: () => { void activate(); } }, text(session?.busy === 'refreshing' ? 'refreshing' : 'refresh')) : null),
          h('p', { style: styles.muted }, text('platformHelp'))),
        h('fieldset', { style: { ...styles.stack, border: 0, margin: 0, padding: 0, minWidth: 0 }, disabled: busy || !sessionId },
          h('legend', { style: { ...styles.heading, marginBottom: 10 } }, text('platform')),
          h('div', { style: { display: 'grid', gridTemplateColumns: 'repeat(2, minmax(0, 1fr))', gap: 8 } },
            ['android', 'ios'].map(value => h('label', { key: value, style: { ...styles.card, ...styles.stack, gap: 10,
              cursor: busy ? 'default' : 'pointer', minWidth: 0,
              borderColor: platform === value ? 'var(--dsw-alias-label-primary)' : 'var(--dsw-alias-border-l2)' } },
            h('div', { style: { ...styles.row, justifyContent: 'flex-start' } },
              h('input', { type: 'radio', name: `mobile-platform-${sessionId}`, value, checked: platform === value,
                onChange: () => { if (platform !== value) void selectPlatform(value); }, style: { margin: 0 } }),
              h('strong', null, text(value))),
            h('span', { style: { ...styles.muted, overflowWrap: 'anywhere' } }, text(value === 'ios' ? 'iosHelp' : 'androidHelp'))))),
          session?.platformSource ? h('p', { role: 'status', style: styles.muted },
            text(session.busy === 'selecting' ? 'selecting' : session.platformSource === 'agent' ? 'agentPlatform' : 'userPlatform')) : null),
        h(ErrorMessage, { error, t }),
        session?.notice && session.notice !== 'disconnected'
          ? h('p', { role: 'status', style: styles.muted }, text(session.notice)) : null,
        platform === 'ios' && !session?.platformAvailable ? h('div', { style: { ...styles.card, ...styles.stack }, role: 'status' },
          h('strong', null, text('iosPending')), h('p', { style: styles.muted }, text('iosPendingHelp'))) : null,
        session?.platformAvailable ? h(React.Fragment, null,
          h('div', { style: { ...styles.card, ...styles.stack } },
            h('span', { style: styles.muted }, text('host')),
            h('strong', { style: { overflowWrap: 'anywhere' } }, state.host || text('waitingHost')),
            h('p', { style: styles.muted }, text('hostHelp'))),
          h('fieldset', { style: { ...styles.stack, border: 0, margin: 0, padding: 0, minWidth: 0 }, disabled: busy },
          h('legend', { style: { ...styles.heading, marginBottom: 10 } }, text('devices')),
          devices.length === 0 ? h('div', { style: { ...styles.card, ...styles.stack } },
            h('strong', null, text('empty')), h('p', { style: styles.muted }, text(platform === 'ios' ? 'iosEmptyHelp' : 'emptyHelp'))) :
            devices.map(item => {
              const status = ['online', 'offline', 'unauthorized', 'stopped'].includes(item.state) ? item.state : 'unknown';
              const kind = ['emulator', 'physical', 'simulator'].includes(item.kind) ? item.kind : 'unknown';
              return h('label', { key: item.id, style: { ...styles.card, display: 'flex', alignItems: 'flex-start', gap: 10, cursor: busy ? 'default' : 'pointer', borderColor: selected === item.id ? 'var(--dsw-alias-label-primary)' : 'var(--dsw-alias-border-l2)' } },
                h('input', { type: 'radio', name: `mobile-device-${sessionId}`, value: item.id, checked: selected === item.id, onChange: () => setSelected(item.id), style: { marginTop: 3 } }),
                h('div', { style: { ...styles.stack, gap: 4, flex: 1, minWidth: 0 } },
                  h('div', { style: styles.row }, h('strong', { style: { overflowWrap: 'anywhere' } }, item.name),
                    h('span', { style: { ...styles.muted, display: 'flex', alignItems: 'center', gap: 5, flexShrink: 0 } }, h(StateDot, { state: status === 'online' ? 'done' : status === 'unauthorized' ? 'warning' : 'idle' }), text(status))),
                  h('span', { style: styles.muted }, `${text(item.platform === 'ios' ? 'ios' : 'android')} · ${text(kind)}`),
                  h('code', { style: styles.detail }, item.serial || item.avd || item.id)));
            })),
        h('div', { style: styles.stack },
          device?.state === 'stopped' && (device.avd || (device.platform === 'ios' && device.kind === 'simulator' && device.serial))
            ? h(React.Fragment, null,
              h(Button, { variant: 'primary', disabled: busy, onClick: () => {
                void (device.platform === 'ios' ? startSimulator(device.serial) : start(device.avd)).then(result => { if (result) setSelected(result.id); });
              } }, text(device.platform === 'ios' ? session?.busy === 'starting' ? 'simulatorStarting' : 'simulatorStart' : session?.busy === 'starting' ? 'starting' : 'start')),
              h('p', { style: styles.muted }, text(device.platform === 'ios' ? 'simulatorConsent' : 'bootConsent')))
            : h(React.Fragment, null,
              h(Button, { variant: 'primary', disabled: busy || selectedState !== 'online', onClick: () => { void connect(selected); } }, text(session?.busy === 'connecting' ? 'connecting' : 'connect')),
              h('p', { style: styles.muted }, text(!device ? 'choose' : selectedState === 'unauthorized' ? 'unauthorizedHelp' : selectedState === 'offline' ? 'offlineHelp' : selectedState === 'online' ? 'retained' : 'unsupported')))),
        state.warnings.length ? h('details', { style: styles.muted }, h('summary', null, text('warnings')),
          h('ul', { style: { paddingLeft: 18 } }, state.warnings.map((warning, index) => h('li', { key: index }, warning)))) : null,
        h('div', { style: { ...styles.stack, marginTop: 'auto', paddingTop: 16, borderTop: '1px solid var(--dsw-alias-border-l2)' } },
          h('strong', { style: { fontSize: 12 } }, text(platform === 'ios' ? 'iosPhase' : 'phase')), h('p', { style: styles.muted }, text(platform === 'ios' ? 'iosPreviewHelp' : 'previewUnavailable')))) : null);
    }

    function apply(ctx) {
      const open = sessionId => {
        if (!sessionId || ctx.sidebarRight.mounted.getSnapshot() !== sessionId) return false;
        return ctx.sidebarRight.openTab(KIND) !== false;
      };
      const controller = createController({ mountedSession: () => ctx.sidebarRight.mounted.getSnapshot(), openPlatform: open });
      const injectedBySession = new Map();
      const beginWidthFit = config => createWidthFit(typeof ctx.layout?.requestRightbarWidth === 'function'
        ? width => ctx.layout.requestRightbarWidth(width) : null, config);
      ctx.effect(() => () => controller.close(), 'mobile-preview.lifecycle');
      ctx.effect(() => ctx.sidebarRight.mounted.subscribe?.(() => { void controller.pollPlatform(); }), 'mobile-preview.platform-selection');
      ctx.effect(() => ctx.locale.register(NS, { en, zh }), 'mobile-preview.locale');
      const t = ctx.locale.bind(NS);
      ctx.effect(() => ctx.sidebarRightTabs.register({ id: ID, kind: KIND, keepMounted: true,
        title: () => t('title'), guide: [{ id: 'devices', order: 35, title: () => t('title'), description: () => t('guide'), icon: DeviceIcon }],
      }), 'mobile-preview.tab');
      ctx.effect(() => ctx.sidebarRight.registerCloseHandler(KIND, sessionId => {
        // Stop capture and release pressed input, while keeping the chat-owned lease.
        void controller.stopPreview(sessionId);
      }), 'mobile-preview.close-handler');
      for (const name of ['conversation.session.header.utilities', 'conversation.input.left']) {
        ctx.effect(() => ctx.slots.inject(name, () => ctx.slots.register({
          name, id: ID, order: 35, locale: NS, inject: () => ({ open }),
        }, EntryButton)), `mobile-preview.${name}`);
      }
      ctx.effect(() => ctx.slots.inject('sidebar.right.pane.tab', () => ctx.slots.register({
        name: 'sidebar.right.pane.tab', key: ID, locale: NS,
        inject: sessionId => {
          if (!injectedBySession.has(sessionId)) injectedBySession.set(sessionId, {
            hooks: { preview: controller.source },
            activate: () => controller.activate(sessionId),
            selectPlatform: platform => controller.selectPlatform(sessionId, platform),
            connect: device => controller.connect(sessionId, device),
            disconnect: () => controller.disconnect(sessionId),
            start: avd => controller.start(sessionId, avd),
            startSimulator: udid => controller.startSimulator(sessionId, udid),
            startPreview: canvas => controller.startPreview(sessionId, canvas),
            stopPreview: () => controller.stopPreview(sessionId),
            mountPreview: (canvas, signal) => controller.mountPreview(sessionId, canvas, signal),
            pausePreview: () => controller.pausePreview(sessionId),
            resumePreview: () => controller.resumePreview(sessionId),
            pressKey: code => controller.pressKey(sessionId, code),
            beginWidthFit,
          });
          return injectedBySession.get(sessionId);
        },
      }, Panel)), 'mobile-preview.body');
    }

    return { inject: ['slots', 'locale', 'sidebarRight', 'sidebarRightTabs', 'layout'], apply };
  },
});
