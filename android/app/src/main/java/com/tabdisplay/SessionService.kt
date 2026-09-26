package com.tabdisplay

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder
import android.os.PowerManager

/**
 * Keeps the process alive (foreground priority + partial wake lock) while a session is open, so locking the
 * tablet or switching apps doesn't get the connection killed. The connection itself lives in [Stream].
 */
class SessionService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Sessão ativa", NotificationManager.IMPORTANCE_LOW))
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val notification = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle("TabDisplay conectado")
            .setContentText("Conectado a ${intent?.getStringExtra(PC) ?: "PC"}")
            .setContentIntent(open)
            .setOngoing(true)
            .build()
        startForeground(1, notification)
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

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        private const val CHANNEL = "session"
        private const val PC = "pc"

        fun start(context: Context, pcName: String) =
            context.startForegroundService(Intent(context, SessionService::class.java).putExtra(PC, pcName))

        fun stop(context: Context) {
            context.stopService(Intent(context, SessionService::class.java))
        }
    }
}
