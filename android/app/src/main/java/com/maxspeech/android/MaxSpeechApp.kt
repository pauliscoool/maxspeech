package com.maxspeech.android

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.util.Log
import androidx.room.Room
import com.maxspeech.android.data.AppDatabase
import com.maxspeech.android.data.AppProfileEntity
import com.maxspeech.android.data.AuthRepository
import com.maxspeech.android.data.SettingsRepository
import com.maxspeech.android.overlay.FloatingMicController
import com.maxspeech.android.overlay.OverlayService
import com.maxspeech.android.pipeline.DictationController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.File

class MaxSpeechApp : Application() {
    val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    lateinit var db: AppDatabase
        private set
    lateinit var settings: SettingsRepository
        private set
    lateinit var auth: AuthRepository
        private set
    lateinit var dictation: DictationController
        private set
    lateinit var floatingMic: FloatingMicController
        private set

    private val _mainUiResumed = MutableStateFlow(false)
    /** True while MainActivity is visible — hide the system overlay over our own UI. */
    val mainUiResumed: StateFlow<Boolean> = _mainUiResumed.asStateFlow()
    private var mainUiStartedCount = 0

    override fun onCreate() {
        super.onCreate()
        instance = this
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            override fun onActivityResumed(activity: Activity) {
                if (activity is MainActivity) _mainUiResumed.value = true
            }

            override fun onActivityPaused(activity: Activity) {
                // Keep "our UI" true until stopped — IME / permission sheets can pause
                // without leaving MaxSpeech, and must not flash the float over login.
                if (activity is MainActivity && mainUiStartedCount <= 0) {
                    _mainUiResumed.value = false
                }
            }

            override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) = Unit
            override fun onActivityStarted(activity: Activity) {
                if (activity is MainActivity) {
                    mainUiStartedCount += 1
                    _mainUiResumed.value = true
                }
            }

            override fun onActivityStopped(activity: Activity) {
                if (activity is MainActivity) {
                    mainUiStartedCount = (mainUiStartedCount - 1).coerceAtLeast(0)
                    if (mainUiStartedCount == 0) _mainUiResumed.value = false
                }
            }

            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit
            override fun onActivityDestroyed(activity: Activity) = Unit
        })
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            runCatching {
                val file = File(filesDir, "last-crash.txt")
                file.writeText(
                    buildString {
                        appendLine("thread=${thread.name}")
                        appendLine(error.stackTraceToString())
                    },
                )
                Log.e(TAG, "Uncaught crash", error)
            }
            val previous = defaultHandler
            if (previous != null) {
                previous.uncaughtException(thread, error)
            } else {
                Log.e(TAG, "Fatal", error)
                android.os.Process.killProcess(android.os.Process.myPid())
            }
        }
        // One-time wipe: Room schema changed (history failed/error fields) and left
        // installs with a broken identity hash that blocked every launch.
        val runtime = getSharedPreferences("maxspeech_runtime", MODE_PRIVATE)
        if (!runtime.getBoolean("db_schema_v3_reset", false)) {
            Log.i(TAG, "Resetting maxspeech.db for schema v3")
            runCatching { deleteDatabase("maxspeech.db") }
            runtime.edit().putBoolean("db_schema_v3_reset", true).apply()
        }
        // Also clear any leftover Room integrity crash gate from prior builds.
        runCatching { File(filesDir, "last-crash.txt").delete() }

        db = Room.databaseBuilder(this, AppDatabase::class.java, "maxspeech.db")
            .fallbackToDestructiveMigration()
            .fallbackToDestructiveMigrationOnDowngrade()
            .build()
        settings = SettingsRepository(this)
        auth = AuthRepository(this, settings)
        dictation = DictationController(this, db, settings, auth)
        floatingMic = FloatingMicController(this)
        appScope.launch {
            runCatching {
                settings.ensureDefaults()
                seedProfiles()
            }.onFailure { err ->
                Log.e(TAG, "Startup seed failed", err)
                if (err.message?.contains("data integrity") == true) {
                    Log.w(TAG, "Wiping DB after integrity failure")
                    runCatching { db.close() }
                    deleteDatabase("maxspeech.db")
                    db = Room.databaseBuilder(this@MaxSpeechApp, AppDatabase::class.java, "maxspeech.db")
                        .fallbackToDestructiveMigration()
                        .fallbackToDestructiveMigrationOnDowngrade()
                        .build()
                    dictation = DictationController(this@MaxSpeechApp, db, settings, auth)
                    runCatching {
                        settings.ensureDefaults()
                        seedProfiles()
                    }.onFailure { Log.e(TAG, "Startup seed retry failed", it) }
                }
            }
            runCatching { maybeStartKeepAlive() }
                .onFailure { Log.e(TAG, "Keep-alive start failed", it) }
        }
    }

    private suspend fun maybeStartKeepAlive() {
        if (!android.provider.Settings.canDrawOverlays(this)) return
        val user = auth.current() ?: return
        if (user.local) return
        Log.i(TAG, "Process start: OverlayService keep-alive")
        OverlayService.start(this)
    }

    private suspend fun seedProfiles() {
        val dao = db.profileDao()
        if (dao.count() > 0) return
        listOf(
            AppProfileEntity(packagePattern = "com.google.android.gm", tone = "formal"),
            AppProfileEntity(packagePattern = "ch.protonmail.android", tone = "formal"),
            AppProfileEntity(packagePattern = "com.yahoo.mobile.client.android.mail", tone = "casual"),
            AppProfileEntity(packagePattern = "com.whatsapp", tone = "casual"),
            AppProfileEntity(packagePattern = "com.google.android.apps.messaging", tone = "casual"),
            AppProfileEntity(packagePattern = "org.telegram.messenger", tone = "casual"),
            AppProfileEntity(packagePattern = "com.discord", tone = "casual"),
            AppProfileEntity(packagePattern = "com.android.chrome", tone = "default"),
            AppProfileEntity(packagePattern = "com.Slack", tone = "casual"),
            AppProfileEntity(packagePattern = "com.microsoft.teams", tone = "default"),
            AppProfileEntity(packagePattern = "com.instagram.android", tone = "casual"),
            AppProfileEntity(packagePattern = "com.linkedin.android", tone = "formal"),
            AppProfileEntity(packagePattern = "com.twitter.android", tone = "casual"),
            AppProfileEntity(packagePattern = "com.zhiliaoapp.musically", tone = "casual"),
            AppProfileEntity(packagePattern = "com.google.android.apps.docs.editors.docs", tone = "prose"),
            AppProfileEntity(packagePattern = "notion.id", tone = "prose"),
        ).forEach { dao.insert(it) }
    }

    companion object {
        private const val TAG = "MaxSpeech"
        private val defaultHandler = Thread.getDefaultUncaughtExceptionHandler()
        lateinit var instance: MaxSpeechApp
            private set
    }
}
