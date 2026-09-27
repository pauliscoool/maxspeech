package com.maxspeech.android.data

import androidx.room.Dao
import androidx.room.Database
import androidx.room.Entity
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.RoomDatabase
import androidx.room.Update
import kotlinx.coroutines.flow.Flow

@Entity(tableName = "history")
data class HistoryEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val text: String,
    val appName: String,
    val createdAt: Long = System.currentTimeMillis(),
    val enhanced: Boolean = false,
    val edited: Boolean = false,
)

@Entity(tableName = "dictionary")
data class DictionaryEntity(
    @PrimaryKey val word: String,
)

@Entity(tableName = "snippets")
data class SnippetEntity(
    @PrimaryKey val trigger: String,
    val expansion: String,
)

@Entity(tableName = "app_profiles")
data class AppProfileEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val packagePattern: String,
    val titlePattern: String = "",
    val tone: String,
    val enabled: Boolean = true,
)

@Entity(tableName = "usage_events")
data class UsageEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val wordCount: Int,
    val createdAt: Long = System.currentTimeMillis(),
)

@Dao
interface HistoryDao {
    @Query("SELECT * FROM history ORDER BY id DESC LIMIT :limit OFFSET :offset")
    fun observe(limit: Int = 80, offset: Int = 0): Flow<List<HistoryEntity>>

    @Query("SELECT * FROM history ORDER BY id DESC LIMIT :limit")
    suspend fun latest(limit: Int = 20): List<HistoryEntity>

    @Query("SELECT * FROM history WHERE text LIKE '%' || :q || '%' ORDER BY id DESC LIMIT 80")
    suspend fun search(q: String): List<HistoryEntity>

    @Insert
    suspend fun insert(row: HistoryEntity): Long

    @Update
    suspend fun update(row: HistoryEntity)

    @Query("DELETE FROM history WHERE id = :id")
    suspend fun delete(id: Long)

    @Query("SELECT COUNT(*) FROM history")
    suspend fun count(): Int

    @Query("SELECT COUNT(DISTINCT appName) FROM history")
    suspend fun distinctApps(): Int

    @Query("SELECT COUNT(DISTINCT appName) FROM history")
    fun observeDistinctApps(): Flow<Int>

    @Query("SELECT COUNT(*) FROM history WHERE enhanced = 1")
    fun observeEnhanced(): Flow<Int>

    @Query("SELECT COUNT(*) FROM history WHERE edited = 1")
    fun observeEdited(): Flow<Int>
}

@Dao
interface DictionaryDao {
    @Query("SELECT word FROM dictionary ORDER BY word")
    fun observe(): Flow<List<String>>

    @Query("SELECT word FROM dictionary ORDER BY word")
    suspend fun all(): List<String>

    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun insert(row: DictionaryEntity)

    @Query("DELETE FROM dictionary WHERE word = :word")
    suspend fun delete(word: String)
}

@Dao
interface SnippetDao {
    @Query("SELECT * FROM snippets ORDER BY trigger")
    fun observe(): Flow<List<SnippetEntity>>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(row: SnippetEntity)

    @Query("DELETE FROM snippets WHERE trigger = :trigger")
    suspend fun delete(trigger: String)
}

@Dao
interface ProfileDao {
    @Query("SELECT * FROM app_profiles ORDER BY id")
    fun observe(): Flow<List<AppProfileEntity>>

    @Query("SELECT * FROM app_profiles")
    suspend fun all(): List<AppProfileEntity>

    @Insert
    suspend fun insert(row: AppProfileEntity)

    @Update
    suspend fun update(row: AppProfileEntity)

    @Query("SELECT COUNT(*) FROM app_profiles")
    suspend fun count(): Int
}

@Dao
interface UsageDao {
    @Insert
    suspend fun insert(row: UsageEntity)

    @Query("SELECT COALESCE(SUM(wordCount), 0) FROM usage_events WHERE createdAt >= :since")
    suspend fun wordsSince(since: Long): Int

    @Query("SELECT COALESCE(SUM(wordCount), 0) FROM usage_events WHERE createdAt >= :since")
    fun observeWordsSince(since: Long): Flow<Int>
}

@Database(
    entities = [
        HistoryEntity::class,
        DictionaryEntity::class,
        SnippetEntity::class,
        AppProfileEntity::class,
        UsageEntity::class,
    ],
    version = 1,
    exportSchema = false,
)
abstract class AppDatabase : RoomDatabase() {
    abstract fun historyDao(): HistoryDao
    abstract fun dictionaryDao(): DictionaryDao
    abstract fun snippetDao(): SnippetDao
    abstract fun profileDao(): ProfileDao
    abstract fun usageDao(): UsageDao
}
