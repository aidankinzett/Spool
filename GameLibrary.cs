using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace LudusaviWrap
{
    public enum SyncStatus
    {
        Unknown = 0,
        Synced = 1,
        LocalNotSynced = 2,
        CloudNotSynced = 3
    }

    public class GameEntry : INotifyPropertyChanged
    {
        private string? _coverImagePath;
        private DateTime? _lastPlayedAt;
        private SyncStatus _syncStatus;
        private bool _isLanCard;
        private List<LanPeer>? _lanPeers;
        private bool _runAsAdmin;

        [JsonIgnore]
        public bool IsLanCard
        {
            get => _isLanCard;
            set { _isLanCard = value; OnPropertyChanged(); }
        }

        [JsonIgnore]
        public List<LanPeer>? LanPeers
        {
            get => _lanPeers;
            set { _lanPeers = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("id")]
        public string Id { get; set; } = Guid.NewGuid().ToString();

        [JsonPropertyName("game_name")]
        public string GameName { get; set; } = "";

        [JsonPropertyName("exe_path")]
        public string ExePath { get; set; } = "";

        [JsonPropertyName("safe_name")]
        public string SafeName { get; set; } = "";

        [JsonPropertyName("cover_image_path")]
        public string? CoverImagePath
        {
            get => _coverImagePath;
            set { _coverImagePath = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("added_at")]
        public DateTime AddedAt { get; set; } = DateTime.UtcNow;

        [JsonPropertyName("last_played_at")]
        public DateTime? LastPlayedAt
        {
            get => _lastPlayedAt;
            set { _lastPlayedAt = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("launcher_exe_path")]
        public string? LauncherExePath { get; set; }

        [JsonPropertyName("sync_status")]
        public SyncStatus SyncStatus
        {
            get => _syncStatus;
            set { _syncStatus = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("game_folder_path")]
        public string? GameFolderPath { get; set; }

        [JsonPropertyName("run_as_admin")]
        public bool RunAsAdmin
        {
            get => _runAsAdmin;
            set { _runAsAdmin = value; OnPropertyChanged(); }
        }

        public event PropertyChangedEventHandler? PropertyChanged;
        private void OnPropertyChanged([CallerMemberName] string? name = null)
            => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
    }

    [JsonSourceGenerationOptions(WriteIndented = true)]
    [JsonSerializable(typeof(List<GameEntry>))]
    internal partial class LibrarySourceGenerationContext : JsonSerializerContext { }

    public class GameLibrary
    {
        private static readonly string AppDataFolder = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ludusavi-wrap");

        private static readonly string LibraryPath = Path.Combine(AppDataFolder, "library.json");

        public List<GameEntry> Entries { get; private set; } = new();

        public GameLibrary()
        {
            Load();
        }

        private void Load()
        {
            try
            {
                if (File.Exists(LibraryPath))
                {
                    string json = File.ReadAllText(LibraryPath);
                    var loaded = JsonSerializer.Deserialize(json, LibrarySourceGenerationContext.Default.ListGameEntry);
                    if (loaded != null)
                        Entries = loaded;
                }
            }
            catch (Exception ex)
            {
                App.Log($"GameLibrary.Load failed: {ex.Message}");
            }
        }

        public void Save()
        {
            try
            {
                Directory.CreateDirectory(AppDataFolder);
                string json = JsonSerializer.Serialize(Entries, LibrarySourceGenerationContext.Default.ListGameEntry);
                string tmpPath = LibraryPath + ".tmp";
                File.WriteAllText(tmpPath, json);
                if (File.Exists(LibraryPath))
                    File.Replace(tmpPath, LibraryPath, LibraryPath + ".bak");
                else
                    File.Move(tmpPath, LibraryPath);
            }
            catch (Exception ex)
            {
                App.Log($"GameLibrary.Save failed: {ex.Message}");
            }
        }

        public void Add(GameEntry entry)
        {
            Entries.Add(entry);
            Save();
        }

        public void Remove(string id)
        {
            Entries.RemoveAll(e => e.Id == id);
            Save();
        }

        public void Update(GameEntry entry)
        {
            int idx = Entries.FindIndex(e => e.Id == entry.Id);
            if (idx >= 0)
            {
                Entries[idx] = entry;
                Save();
            }
        }

        public GameEntry? FindByName(string gameName)
            => Entries.Find(e => string.Equals(e.GameName, gameName, StringComparison.OrdinalIgnoreCase));
    }
}
