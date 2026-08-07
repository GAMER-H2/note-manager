package com.mh968.note_manager

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Build
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class StartSyncArgs {
  var timeoutMs: Long = 3L * 60 * 1000
}

/**
 * Bridges Rust <-> the sync foreground service. Command names ("startSync" /
 * "stopSync") must match the strings passed to run_mobile_plugin in Rust.
 */
@TauriPlugin
class SyncServicePlugin(private val activity: Activity) : Plugin(activity) {
  // The application context, deliberately: the Activity can be destroyed (task
  // swiped) while the service is still running, and we still need a valid
  // Context to stop it.
  private val ctx: Context = activity.applicationContext

  @Command
  fun startSync(invoke: Invoke) {
    val args = invoke.parseArgs(StartSyncArgs::class.java)
    val intent = Intent(ctx, SyncForegroundService::class.java).apply {
      putExtra(SyncForegroundService.EXTRA_TIMEOUT_MS, args.timeoutMs)
    }
    try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        ctx.startForegroundService(intent)
      } else {
        ctx.startService(intent)
      }
      invoke.resolve()
    } catch (e: Exception) {
      // ForegroundServiceStartNotAllowedException (API 31+, app in background)
      // or MissingForegroundServiceTypeException (API 34+). Report, don't
      // crash — the sync itself can still run.
      invoke.reject("startForegroundService failed: ${e.message}")
    }
  }

  @Command
  fun stopSync(invoke: Invoke) {
    try {
      ctx.stopService(Intent(ctx, SyncForegroundService::class.java))
    } catch (_: Exception) {
      // Already gone.
    }
    invoke.resolve()
  }
}
