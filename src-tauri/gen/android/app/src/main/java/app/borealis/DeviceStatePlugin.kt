package app.borealis

import android.app.Activity
import android.app.ActivityManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * Device facts for semantic search (local-search-spec sec. 7, backend `semantic_device.rs`): total RAM for
 * the tier probe (`ActivityManager.MemoryInfo.totalMem`) and the state the indexing scheduler pauses on
 * (charging and battery level from the sticky `ACTION_BATTERY_CHANGED`, Battery Saver, thermal status on
 * API 29+). Answers `{ totalMem, charging, batteryPercent, powerSave, thermalStatus }`; -1 means unknown.
 * Read-only, no permissions needed, nothing leaves the device.
 *
 * B-48 O13: `hapticFeedbackEnabled` answers `{ enabled }` from Settings > Sound & vibration > Touch feedback
 * (`Settings.System.HAPTIC_FEEDBACK_ENABLED`), because the haptics plugin's `Vibrator` calls do not apply it themselves.
 */
@TauriPlugin
class DeviceStatePlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun deviceState(invoke: Invoke) {
    try {
      val out = JSObject()
      val am = activity.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
      val mem = ActivityManager.MemoryInfo()
      am.getMemoryInfo(mem)
      out.put("totalMem", mem.totalMem)

      val battery = activity.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
      val status = battery?.getIntExtra(BatteryManager.EXTRA_STATUS, -1) ?: -1
      val plugged = battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0
      val level = battery?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
      val scale = battery?.getIntExtra(BatteryManager.EXTRA_SCALE, -1) ?: -1
      val charging = plugged != 0 ||
        status == BatteryManager.BATTERY_STATUS_CHARGING ||
        status == BatteryManager.BATTERY_STATUS_FULL
      out.put("charging", charging)
      out.put("batteryPercent", if (level >= 0 && scale > 0) level * 100 / scale else -1)

      val pm = activity.getSystemService(Context.POWER_SERVICE) as PowerManager
      out.put("powerSave", pm.isPowerSaveMode)
      val thermal = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) pm.currentThermalStatus else -1
      out.put("thermalStatus", thermal)
      invoke.resolve(out)
    } catch (e: Exception) {
      invoke.reject(e.message ?: "Device state is not available.")
    }
  }

  @Command
  fun hapticFeedbackEnabled(invoke: Invoke) {
    val out = JSObject()
    val enabled = try {
      Settings.System.getInt(activity.contentResolver, Settings.System.HAPTIC_FEEDBACK_ENABLED, 1) != 0
    } catch (e: Exception) {
      true
    }
    out.put("enabled", enabled)
    invoke.resolve(out)
  }
}
