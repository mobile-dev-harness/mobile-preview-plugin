package dev.mpp;

import android.os.Looper;

/** Android VM entry only; capture, encoding, transport, and input remain native. */
public final class Bootstrap {
    private Bootstrap() {}

    private static native int run();

    public static void main(String[] args) {
        int status = 1;
        try {
            if (args.length != 1) {
                throw new IllegalArgumentException("Expected one native library path");
            }
            if (Looper.myLooper() == null) {
                Looper.prepareMainLooper();
            }
            System.load(args[0]);
            status = run();
        } catch (Throwable error) {
            System.err.println("MPP bootstrap failed: " + error.getClass().getName());
        }
        System.exit(status);
    }
}
