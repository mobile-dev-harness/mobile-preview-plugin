import { accessSync, constants, lstatSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const PACKAGE_ROOT = fileURLToPath(new URL('../../', import.meta.url));

function failure(code, message, hint) {
  return Object.assign(new Error(`mobile-preview: ${message} ${hint}`), { code, hint });
}

function requireFile(path, executable, externalAssets = false) {
  try {
    if (!lstatSync(path).isFile()) throw new Error('not a regular file');
    accessSync(path, constants.R_OK | (executable ? constants.X_OK : 0));
  } catch {
    if (externalAssets) {
      throw failure('INVALID_CONFIG', 'The configured Android device assets are missing or unreadable.',
        'Set deviceAssets to a directory containing bootstrap.jar and libmpp_android_device.so.');
    }
    throw failure('INCOMPLETE_PACKAGE', 'The bundled runtime is incomplete or has invalid file permissions.',
      'Reinstall the macOS arm64 preview package, or configure an absolute executable path for a development build.');
  }
}

/** Resolve installed assets relative to this package, never the Host working directory. */
export function resolveRuntime(input, {
  packageRoot = PACKAGE_ROOT, platform = process.platform, arch = process.arch,
} = {}) {
  // A developer-supplied host can discover devices without having preview assets.
  if (input.executable !== undefined) return {
    executable: input.executable,
    ...(input.deviceAssets === undefined ? {} : { deviceAssets: input.deviceAssets }),
  };
  if (platform !== 'darwin' || arch !== 'arm64') {
    throw failure('UNSUPPORTED_HOST', 'The bundled preview runtime supports macOS Apple Silicon only.',
      'Use a supported Host, or configure an absolute executable path for a compatible development build.');
  }
  const executable = join(packageRoot, 'native/darwin-arm64/mpp');
  const deviceAssets = input.deviceAssets ?? join(packageRoot, 'native/android-arm64');
  requireFile(executable, true);
  for (const name of ['bootstrap.jar', 'libmpp_android_device.so']) {
    requireFile(join(deviceAssets, name), false, input.deviceAssets !== undefined);
  }
  return { executable, deviceAssets };
}
