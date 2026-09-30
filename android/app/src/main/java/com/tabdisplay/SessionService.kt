package com.tabdisplay

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.pm.ServiceInfo
import android.content.Context
import android.content.Intent
import android.os.IBinder
import android.os.PowerManager
import android.os.Build
import androidx.annotation.RequiresApi

/**
 * Keeps the process alive (foreground priority + partial wake lock) while a session is open, so locking the
 * tablet or switching apps doesn't get the connection killed. The connection itself lives in [Stream].
 */
class SessionService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, getString(R.string.session_channel), NotificationManager.IMPORTANCE_LOW))
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val notification = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(getString(R.string.session_title))
            .setContentText(getString(R.string.session_text, intent?.getStringExtra(PC) ?: getString(R.string.badge_pc)))
            .setContentIntent(open)
            .setOngoing(true)
            .build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(1, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        } else {
            startForeground(1, notification)
        }
        if (intent?.action == PAUSE) {
            wakeLock?.release()
            wakeLock = null
            return START_NOT_STICKY
        }
        if (wakeLock == null) {
            wakeLock = getSystemService(PowerManager::class.java)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "tabdisplay:session").apply { acquire() }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        wakeLock?.release()
        wakeLock = null
    }

    /** Android 15 dataSync services must stop promptly when their cumulative timeout fires. */
    @RequiresApi(Build.VERSION_CODES.VANILLA_ICE_CREAM)
    override fun onTimeout(startId: Int, fgsType: Int) {
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf(startId)
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        private const val CHANNEL = "session"
        private const val PC = "pc"
        private const val PAUSE = "com.tabdisplay.PAUSE_SESSION"

        fun start(context: Context, pcName: String) =
            context.startForegroundService(Intent(context, SessionService::class.java).putExtra(PC, pcName))

        fun stop(context: Context) {
            context.stopService(Intent(context, SessionService::class.java))
        }

        fun pause(context: Context) {
            context.startService(Intent(context, SessionService::class.java).setAction(PAUSE))
        }

        fun resume(context: Context, pcName: String) {
            start(context, pcName)
        }
    }
}
