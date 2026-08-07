package com.mh968.note_manager

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/**
 * Short-lived foreground service that keeps the app process alive for the
 * duration of a sync, so a swipe-away mid-sync doesn't lose the run. Started
 * and stopped from Rust via SyncServicePlugin.
 */
class SyncForegroundService : Service() {
  companion object {
    const val ACTION_STOP = "com.mh968.note_manager.action.SYNC_STOP"
    const val EXTRA_TIMEOUT_MS = "timeoutMs"

    private const val CHANNEL_ID = "sync_fgs"
    private const val NOTIFICATION_ID = 0x5117
    private const val DEFAULT_TIMEOUT_MS = 3L * 60 * 1000
  }

  private val watchdog = Handler(Looper.getMainLooper())
  private val stopRunnable = Runnable { shutdown() }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      shutdown()
      return START_NOT_STICKY
    }

    // Must reach startForeground() within ~5s, so do nothing expensive first.
    ensureChannel()
    ServiceCompat.startForeground(
      this,
      NOTIFICATION_ID,
      buildNotification(),
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
      } else {
        0
      },
    )

    // Backstop: if the stop call never lands (Activity destroyed by a swipe,
    // JNI pipe stalled), self-destruct so the notification can't linger.
    val timeout = intent?.getLongExtra(EXTRA_TIMEOUT_MS, DEFAULT_TIMEOUT_MS) ?: DEFAULT_TIMEOUT_MS
    watchdog.removeCallbacks(stopRunnable)
    watchdog.postDelayed(stopRunnable, timeout)

    return START_NOT_STICKY // never auto-restart after process death
  }

  /**
   * Android 15+ (API 35): the dataSync time budget is exhausted. Stop within a
   * few seconds or the system throws ForegroundServiceDidNotStopInTimeException.
   */
  override fun onTimeout(startId: Int, fgsType: Int) {
    shutdown()
  }

  override fun onDestroy() {
    watchdog.removeCallbacks(stopRunnable)
    // stopService() lands here directly (not via shutdown), so make sure the
    // notification is torn down promptly rather than relying on auto-removal.
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    super.onDestroy()
  }

  private fun shutdown() {
    watchdog.removeCallbacks(stopRunnable)
    // STOP_FOREGROUND_REMOVE tears the notification down immediately.
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    stopSelf()
  }

  private fun ensureChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    if (nm.getNotificationChannel(CHANNEL_ID) != null) return
    nm.createNotificationChannel(
      NotificationChannel(
        CHANNEL_ID,
        "Background sync",
        NotificationManager.IMPORTANCE_MIN, // no status-bar icon, no sound
      ).apply {
        description = "Keeps a sync running when the app is closed"
        setShowBadge(false)
        setSound(null, null)
        enableVibration(false)
        enableLights(false)
        lockscreenVisibility = Notification.VISIBILITY_SECRET
      },
    )
  }

  private fun buildNotification(): Notification =
    NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.stat_notify_sync)
      .setContentTitle("Syncing notes")
      .setPriority(NotificationCompat.PRIORITY_MIN)
      .setCategory(NotificationCompat.CATEGORY_SERVICE)
      .setSilent(true)
      .setOngoing(true)
      .setShowWhen(false)
      .setLocalOnly(true)
      // DEFERRED (the API 31+ default) hides the notification for the first
      // ~10s, so a sync that finishes quickly shows the user nothing at all.
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_DEFERRED)
      .build()
}
