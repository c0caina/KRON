package io.github.c0caina.kron;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.IOException;

/** Keeps the local harness running in the foreground service. */
public class KronService extends Service {
    static final String STOP = "stop";
    private volatile boolean running;
    private volatile Process proc;
    static File logFile(android.content.Context c) {
        return new File(c.getFilesDir(), "kron.log");
    }

    @Override
    public void onCreate() {
        super.onCreate();
        NotificationManager nm = getSystemService(NotificationManager.class);
        nm.createNotificationChannel(new NotificationChannel("kron", "kron", NotificationManager.IMPORTANCE_LOW));
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class),
                PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stop = PendingIntent.getService(this, 1, new Intent(this, KronService.class).setAction(STOP),
                PendingIntent.FLAG_IMMUTABLE);
        Notification n = new Notification.Builder(this, "kron")
                .setSmallIcon(R.drawable.ic_stat)
                .setContentTitle("kron работает")
                .setContentText("Агент работает на телефоне")
                .setContentIntent(open)
                .addAction(new Notification.Action.Builder(null, "Остановить", stop).build())
                .setOngoing(true)
                .build();
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        } else {
            startForeground(1, n);
        }
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && STOP.equals(intent.getAction())) {
            stopSelf();
            return START_NOT_STICKY;
        }
        if (!running) {
            running = true;
            new Thread(this::loop, "kron").start();
        }
        return START_STICKY;
    }

    private void loop() {
        File home = getFilesDir();
        File work = new File(home, "work");
        work.mkdirs();
        File runtime = new File(home, "runtime");
        try {
            // APK updates never overwrite the agent's files. Seed just once.
            File marker = new File(home, "runtime-initialized");
            if (!marker.exists()) {
                seed("runtime", runtime);
                if (!marker.createNewFile()) throw new IOException("Cannot mark runtime initialized");
            }
        } catch (Exception e) {
            logError(e);
            running = false;
            return;
        }
        while (running) {
            try {
                File log = logFile(this);
                if (log.length() > 1_000_000) log.delete();
                ProcessBuilder pb = new ProcessBuilder("su", "-c",
                        "exec /system/bin/sh " + quote(new File(runtime, "start.sh").getAbsolutePath()))
                        .directory(work)
                        .redirectErrorStream(true)
                        .redirectOutput(ProcessBuilder.Redirect.appendTo(log));
                new File(runtime, "pid").delete();
                proc = pb.start();
                proc.waitFor();
            } catch (Exception e) {
                logError(e);
            }
            if (running) {
                try {
                    Thread.sleep(3000);
                } catch (InterruptedException e) {
                    return;
                }
            }
        }
    }

    private static String quote(String value) {
        return "'" + value.replace("'", "'\"'\"'") + "'";
    }

    private void seed(String asset, File destination) throws IOException {
        String[] children = getAssets().list(asset);
        if (children != null && children.length > 0) {
            if (!destination.isDirectory() && !destination.mkdirs()) throw new IOException("Cannot create " + destination);
            for (String child : children) seed(asset + "/" + child, new File(destination, child));
        } else if (!destination.exists()) {
            File temporary = new File(destination.getPath() + ".seed");
            try (InputStream in = getAssets().open(asset); FileOutputStream out = new FileOutputStream(temporary)) {
                byte[] buffer = new byte[8192];
                int n;
                while ((n = in.read(buffer)) != -1) out.write(buffer, 0, n);
            }
            if ((destination.getName().equals("kron") || destination.getName().equals("llama-server"))
                    && !temporary.setExecutable(true, true))
                throw new IOException("Cannot mark runtime executable");
            if (!temporary.renameTo(destination)) throw new IOException("Cannot install " + destination);
        }
    }

    private void logError(Exception e) {
        try (java.io.FileWriter log = new java.io.FileWriter(logFile(this), true)) {
            log.write(e.toString() + "\n");
        } catch (Exception ignored) {}
    }

    @Override
    public void onDestroy() {
        running = false;
        Process p = proc;
        try {
            if (p == null || !p.isAlive()) throw new IOException("Runtime is not running");
            File pid = new File(getFilesDir(), "runtime/pid");
            String value = new String(java.nio.file.Files.readAllBytes(pid.toPath()), java.nio.charset.StandardCharsets.UTF_8).trim();
            if (value.matches("[0-9]+")) new ProcessBuilder("su", "-c", "kill " + value).start();
        } catch (Exception ignored) {}
        if (p != null) p.destroy();
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
