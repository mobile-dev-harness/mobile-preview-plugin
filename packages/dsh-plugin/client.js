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
      title: 'Mobile devices', open: 'Connect a mobile device', guide: 'Connect an emulator or Android device',
      host: 'Connected host', waitingHost: 'Host not connected', hostHelp: 'Devices belong to the machine running DSH.',
      refresh: 'Refresh', refreshing: 'Refreshing…', devices: 'Choose a device', empty: 'No devices found',
      emptyHelp: 'Connect an Android device with USB debugging enabled, or create an AVD in Android Studio, then refresh.',
      online: 'Online', offline: 'Offline', unauthorized: 'Authorization required', stopped: 'Stopped', unknown: 'Unknown',
      emulator: 'Emulator', physical: 'Physical device', simulator: 'Simulator', android: 'Android', ios: 'iOS',
      connect: 'Connect', disconnect: 'Disconnect', start: 'Start selected emulator', starting: 'Starting emulator…',
      connecting: 'Connecting…', disconnecting: 'Disconnecting…', choose: 'Select a device to connect.',
      bootConsent: 'Starting opens the selected AVD on the connected host.',
      offlineHelp: 'Reconnect the device and refresh before connecting.',
      unauthorizedHelp: 'Unlock the device and accept its USB debugging prompt, then refresh.',
      current: 'This chat’s connection', connected: 'Transport ready', disconnected: 'Disconnected',
      uncertain: 'Checking connection', retained: 'The connection stays with this chat when you switch chats or close this panel.',
      phase: 'Device connection is available', previewUnavailable: 'Live preview and touch input are not available in this version.',
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
      title: '移动设备', open: '连接移动设备', guide: '连接模拟器或 Android 真机',
      host: '连接主机', waitingHost: '尚未连接主机', hostHelp: '设备位于运行 DSH 的这台主机上。',
      refresh: '刷新', refreshing: '正在刷新…', devices: '选择设备', empty: '未发现设备',
      emptyHelp: '连接已开启 USB 调试的 Android 设备，或在 Android Studio 中创建 AVD，然后刷新。',
      online: '在线', offline: '离线', unauthorized: '需要授权', stopped: '未启动', unknown: '未知',
      emulator: '模拟器', physical: '真机', simulator: '模拟器', android: 'Android', ios: 'iOS',
      connect: '连接', disconnect: '断开连接', start: '启动所选模拟器', starting: '正在启动模拟器…',
      connecting: '正在连接…', disconnecting: '正在断开…', choose: '请选择要连接的设备。',
      bootConsent: '将在连接主机上启动你选择的 AVD。',
      offlineHelp: '请重新连接设备，刷新后再连接。',
      unauthorizedHelp: '请解锁设备并确认 USB 调试授权，然后刷新。',
      current: '此聊天的连接', connected: '设备通道已就绪', disconnected: '已断开',
      uncertain: '正在核实连接', retained: '切换聊天或关闭面板后，连接仍归属此聊天。',
      phase: '已支持设备连接', previewUnavailable: '当前版本暂未提供实时投屏和触控输入。',
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

    function createController() {
      const source = createSnapshotStore({ host: null, devices: [], warnings: [], error: null, sessions: {} });
      const bindings = new Map();
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
      const networkMargin = 2000;
      const patch = update => source.update(update);
      const row = (state, sessionId) => state.sessions[sessionId] ??= {
        binding: null, session: null, busy: null, error: null, notice: null, verified: false,
      };

      async function request(method, params) {
        if (closed) throw localError('clientClosed');
        const abort = new AbortController();
        pending.add(abort);
        const timer = setTimeout(() => abort.abort(),
          (method === 'emulator.start' ? bootTimeout : requestTimeout) + networkMargin);
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
        bindings.delete(sessionId);
        patch(state => Object.assign(row(state, sessionId), { binding: null, session: null, verified: false, notice }));
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
              || !['leaseTtlMs', 'heartbeatMs', 'requestTimeoutMs', 'bootTimeoutMs'].every(
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

      async function call(method, params) {
        const token = await ensureClient();
        const current = generation;
        const result = await request(method, { client: token, ...params });
        if (closed || current !== generation || token !== client) throw localError('stale');
        return result;
      }

      function accept(sessionId, result) {
        if (!result) { forget(sessionId, null); return; }
        if (typeof result.binding !== 'string' || !result.session?.device) throw localError('invalidResponse');
        if (result.session.state === 'disconnected') { forget(sessionId, 'stale'); return; }
        bindings.set(sessionId, result.binding);
        patch(state => Object.assign(row(state, sessionId), {
          binding: result.binding, session: result.session, verified: true, notice: null, error: null,
        }));
      }

      async function inventory() {
        const result = await call('devices.list', {});
        if (!Array.isArray(result?.devices) || !Array.isArray(result.warnings)) throw localError('invalidResponse');
        patch(state => { state.devices = result.devices; state.warnings = result.warnings; state.error = null; });
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
        await inventory();
        const result = await call('session.list', { sessionId });
        if (result) accept(sessionId, result);
        else if (bindings.has(sessionId)) forget(sessionId, 'stale');
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

      const wake = () => { if (document.visibilityState !== 'hidden') void heartbeat(); };
      document.addEventListener('visibilitychange', wake);
      window.addEventListener('pageshow', wake);
      return {
        source,
        activate,
        connect: (sessionId, device) => run(sessionId, 'connecting', async () => {
          accept(sessionId, await call('session.connect', { sessionId, device }));
        }),
        disconnect: sessionId => run(sessionId, 'disconnecting', async () => {
          const binding = bindings.get(sessionId);
          if (binding) await call('session.disconnect', { binding });
          forget(sessionId, 'disconnected');
        }),
        start: (sessionId, avd) => run(sessionId, 'starting', async () => {
          const device = await call('emulator.start', { avd, consent: true });
          await inventory();
          return device;
        }),
        close() {
          if (closed) return;
          closed = true;
          generation += 1;
          clearInterval(timer);
          document.removeEventListener('visibilitychange', wake);
          window.removeEventListener('pageshow', wake);
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

    function Panel({ sessionId, usePreview, useTabInfo, activate, connect, disconnect, start, t }) {
      const state = usePreview(value => value);
      const { tab } = useTabInfo();
      const session = state.sessions[sessionId];
      const [selected, setSelected] = React.useState('');
      React.useEffect(() => {
        if (tab.visible && !tab.signal.aborted) void activate();
      }, [sessionId, tab.visible, tab.signal, activate]);
      const device = state.devices.find(item => item.id === selected);
      const busy = Boolean(session?.busy);
      const connected = Boolean(session?.binding);
      const selectedState = ['online', 'offline', 'unauthorized', 'stopped'].includes(device?.state) ? device.state : 'unknown';
      const error = session?.error || state.error;
      const text = key => t(key);
      return h('section', { style: styles.panel, 'aria-label': text('title'), 'data-mobile-preview': '' },
        h('div', { style: styles.stack },
          h('div', { style: styles.row }, h('h2', { style: styles.heading }, text('title')),
            h(Button, { size: 'sm', variant: 'ghost', disabled: busy, onClick: () => { void activate(); } }, text(session?.busy === 'refreshing' ? 'refreshing' : 'refresh'))),
          h('div', { style: { ...styles.card, ...styles.stack } },
            h('span', { style: styles.muted }, text('host')),
            h('strong', { style: { overflowWrap: 'anywhere' } }, state.host || text('waitingHost')),
            h('p', { style: styles.muted }, text('hostHelp')))),
        h(ErrorMessage, { error, t }),
        session?.notice && session.notice !== 'disconnected'
          ? h('p', { role: 'status', style: styles.muted }, text(session.notice)) : null,
        h('fieldset', { style: { ...styles.stack, border: 0, margin: 0, padding: 0, minWidth: 0 }, disabled: busy || connected },
          h('legend', { style: { ...styles.heading, marginBottom: 10 } }, text('devices')),
          state.devices.length === 0 ? h('div', { style: { ...styles.card, ...styles.stack } },
            h('strong', null, text('empty')), h('p', { style: styles.muted }, text('emptyHelp'))) :
            state.devices.map(item => {
              const status = ['online', 'offline', 'unauthorized', 'stopped'].includes(item.state) ? item.state : 'unknown';
              const kind = ['emulator', 'physical', 'simulator'].includes(item.kind) ? item.kind : 'unknown';
              return h('label', { key: item.id, style: { ...styles.card, display: 'flex', alignItems: 'flex-start', gap: 10, cursor: busy || connected ? 'default' : 'pointer', borderColor: selected === item.id ? 'var(--dsw-alias-label-primary)' : 'var(--dsw-alias-border-l2)' } },
                h('input', { type: 'radio', name: `mobile-device-${sessionId}`, value: item.id, checked: selected === item.id, onChange: () => setSelected(item.id), style: { marginTop: 3 } }),
                h('div', { style: { ...styles.stack, gap: 4, flex: 1, minWidth: 0 } },
                  h('div', { style: styles.row }, h('strong', { style: { overflowWrap: 'anywhere' } }, item.name),
                    h('span', { style: { ...styles.muted, display: 'flex', alignItems: 'center', gap: 5, flexShrink: 0 } }, h(StateDot, { state: status === 'online' ? 'done' : status === 'unauthorized' ? 'warning' : 'idle' }), text(status))),
                  h('span', { style: styles.muted }, `${text(item.platform === 'ios' ? 'ios' : 'android')} · ${text(kind)}`),
                  h('code', { style: styles.detail }, item.serial || item.avd || item.id)));
            })),
        !connected ? h('div', { style: styles.stack },
          device?.state === 'stopped' && device.avd
            ? h(React.Fragment, null,
              h(Button, { variant: 'primary', disabled: busy, onClick: () => {
                void start(device.avd).then(result => { if (result) setSelected(result.id); });
              } }, text(session?.busy === 'starting' ? 'starting' : 'start')),
              h('p', { style: styles.muted }, text('bootConsent')))
            : h(React.Fragment, null,
              h(Button, { variant: 'primary', disabled: busy || selectedState !== 'online', onClick: () => { void connect(selected); } }, text(session?.busy === 'connecting' ? 'connecting' : 'connect')),
              h('p', { style: styles.muted }, text(!device ? 'choose' : selectedState === 'unauthorized' ? 'unauthorizedHelp' : selectedState === 'offline' ? 'offlineHelp' : selectedState === 'online' ? 'retained' : 'unsupported')))) :
          h('div', { style: { ...styles.card, ...styles.stack }, role: 'status' },
            h('span', { style: styles.muted }, text('current')),
            h('strong', null, session.session.device.name),
            h('div', { style: { display: 'flex', alignItems: 'center', gap: 7 } },
              h(StateDot, { state: session.verified ? 'done' : 'warning' }), text(session.verified ? 'connected' : 'uncertain')),
            h('p', { style: styles.muted }, text('retained')),
            h(Button, { variant: 'outline', disabled: busy, onClick: () => { void disconnect(); } }, text(session.busy === 'disconnecting' ? 'disconnecting' : 'disconnect'))),
        state.warnings.length ? h('details', { style: styles.muted }, h('summary', null, text('warnings')),
          h('ul', { style: { paddingLeft: 18 } }, state.warnings.map((warning, index) => h('li', { key: index }, warning)))) : null,
        h('div', { style: { ...styles.stack, marginTop: 'auto', paddingTop: 16, borderTop: '1px solid var(--dsw-alias-border-l2)' } },
          h('strong', { style: { fontSize: 12 } }, text('phase')), h('p', { style: styles.muted }, text('previewUnavailable'))));
    }

    function apply(ctx) {
      const controller = createController();
      const injectedBySession = new Map();
      ctx.effect(() => () => controller.close(), 'mobile-preview.lifecycle');
      ctx.effect(() => ctx.locale.register(NS, { en, zh }), 'mobile-preview.locale');
      const t = ctx.locale.bind(NS);
      ctx.effect(() => ctx.sidebarRightTabs.register({ id: ID, kind: KIND, keepMounted: true,
        title: () => t('title'), guide: [{ id: 'devices', order: 35, title: () => t('title'), description: () => t('guide'), icon: DeviceIcon }],
      }), 'mobile-preview.tab');
      ctx.effect(() => ctx.sidebarRight.registerCloseHandler(KIND, () => {
        // Closing presentation must not revoke a chat-owned device lease.
      }), 'mobile-preview.close-handler');
      const open = sessionId => {
        if (sessionId && ctx.sidebarRight.mounted.getSnapshot() === sessionId) ctx.sidebarRight.openTab(KIND);
      };
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
            connect: device => controller.connect(sessionId, device),
            disconnect: () => controller.disconnect(sessionId),
            start: avd => controller.start(sessionId, avd),
          });
          return injectedBySession.get(sessionId);
        },
      }, Panel)), 'mobile-preview.body');
    }

    return { inject: ['slots', 'locale', 'sidebarRight', 'sidebarRightTabs'], apply };
  },
});
