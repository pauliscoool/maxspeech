package com.maxspeech.android

import android.app.Application
import androidx.room.Room
import com.maxspeech.android.data.AppDatabase
import com.maxspeech.android.data.AppProfileEntity
import com.maxspeech.android.data.AuthRepository
import com.maxspeech.android.data.SettingsRepository
import com.maxspeech.android.pipeline.DictationController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

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

    override fun onCreate() {
        super.onCreate()
        instance = this
        db = Room.databaseBuilder(this, AppDatabase::class.java, "maxspeech.db")
            .fallbackToDestructiveMigration()
            .build()
        settings = SettingsRepository(this)
        auth = AuthRepository(this, settings)
        dictation = DictationController(this, db, settings, auth)
        appScope.launch {
            settings.ensureDefaults()
            seedProfiles()
        }
    }

    private suspend fun seedProfiles() {
        val dao = db.profileDao()
        if (dao.count() > 0) return
        listOf(
            AppProfileEntity(packagePattern = "com.google.android.gm", tone = "formal"),
            AppProfileEntity(packagePattern = "com.whatsapp", tone = "casual"),
            AppProfileEntity(packagePattern = "com.google.android.apps.messaging", tone = "casual"),
            AppProfileEntity(packagePattern = "com.android.chrome", tone = "default"),
            AppProfileEntity(packagePattern = "com.slack", tone = "casual"),
            AppProfileEntity(packagePattern = "com.discord", tone = "casual"),
            AppProfileEntity(packagePattern = "com.instagram.android", tone = "casual"),
            AppProfileEntity(packagePattern = "com.linkedin.android", tone = "formal"),
            AppProfileEntity(packagePattern = "org.telegram.messenger", tone = "casual"),
            AppProfileEntity(packagePattern = "com.twitter.android", tone = "casual"),
            AppProfileEntity(packagePattern = "com.google.android.apps.docs.editors.docs", tone = "prose"),
            AppProfileEntity(packagePattern = "com.notion.id", tone = "prose"),
        ).forEach { dao.insert(it) }
    }

    companion object {
        lateinit var instance: MaxSpeechApp
            private set
    }
}
