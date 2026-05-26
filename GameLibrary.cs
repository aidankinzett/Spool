using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace LudusaviWrap
{
    public class GameEntry : INotifyPropertyChanged
    {
        private string? _coverImagePath;
        private string? _heroImagePath;
        private DateTime? _lastPlayedAt;
        private bool _isLanCard;
        private List<LanPeer>? _lanPeers;
        private bool _runAsAdmin;
        private bool _lanShared;
        private int _playtimeMinutes;
        private int _saveBackupCount;
        private DateTime? _saveLastBackedUpAt;
        private double _saveBackupSizeMb;
        private double _installSizeMb;
        private string _installSource = "manual";
        private string? _lanInstallSourceDeviceName;
        private string? _lanInstallSourceDeviceId;
        private bool _canVerify;
        private LanPeer? _verifiedSourcePeer;
        private bool _hasQuickSfv;

        [JsonIgnore]
        public bool IsLanCard
        {
            get => _isLanCard;
            set { _isLanCard = value; OnPropertyChanged(); OnPropertyChanged(nameof(CanVerify)); }
        }

        [JsonIgnore]
        public List<LanPeer>? LanPeers
        {
            get => _lanPeers;
            set { _lanPeers = value; OnPropertyChanged(); OnPropertyChanged(nameof(CanVerify)); }
        }

        [JsonPropertyName("id")]
        public string Id { get; set; } = Guid.NewGuid().ToString();

        [JsonPropertyName("game_name")]
        public string GameName { get; set; } = "";

        [JsonPropertyName("exe_path")]
        public string ExePath { get; set; } = "";

        [JsonPropertyName("safe_name")]
        public string SafeName { get; set; } = "";

        private string? MigratePath(string? path)
        {
            if (string.IsNullOrEmpty(path)) return path;
            if (path.Contains("ludusavi-wrap", StringComparison.OrdinalIgnoreCase))
            {
                string migrated = path.Replace("ludusavi-wrap", "Spool", StringComparison.OrdinalIgnoreCase);
                if (File.Exists(migrated))
                {
                    return migrated;
                }
            }
            return path;
        }

        [JsonPropertyName("cover_image_path")]
        public string? CoverImagePath
        {
            get => MigratePath(_coverImagePath);
            set { _coverImagePath = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("hero_image_path")]
        public string? HeroImagePath
        {
            get => MigratePath(_heroImagePath);
            set { _heroImagePath = value; OnPropertyChanged(); }
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

        [JsonPropertyName("game_folder_path")]
        public string? GameFolderPath { get; set; }

        [JsonPropertyName("run_as_admin")]
        public bool RunAsAdmin
        {
            get => _runAsAdmin;
            set { _runAsAdmin = value; OnPropertyChanged(); }
        }

        // ── Metadata (populated from API at import time) ──────────────────────

        [JsonPropertyName("description")]
        public string Description { get; set; } = "";

        [JsonPropertyName("developer")]
        public string Developer { get; set; } = "";

        [JsonPropertyName("publisher")]
        public string Publisher { get; set; } = "";

        [JsonPropertyName("genres")]
        public List<string> Genres { get; set; } = new();

        [JsonPropertyName("release_date")]
        public DateTime? ReleaseDate { get; set; }

        [JsonPropertyName("install_size_mb")]
        public double InstallSizeMb
        {
            get => _installSizeMb;
            set { _installSizeMb = value; OnPropertyChanged(); }
        }

        // ── Play tracking ──────────────────────────────────────────────────────

        [JsonPropertyName("playtime_minutes")]
        public int PlaytimeMinutes
        {
            get => _playtimeMinutes;
            set { _playtimeMinutes = value; OnPropertyChanged(); }
        }

        // ── LAN sharing ───────────────────────────────────────────────────────

        [JsonPropertyName("lan_shared")]
        public bool LanShared
        {
            get => _lanShared;
            set { _lanShared = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("lan_share_folder")]
        public string? LanShareFolder { get; set; }

        // ── Save backup stats (updated by Ludusavi workflow) ──────────────────

        [JsonPropertyName("save_backup_count")]
        public int SaveBackupCount
        {
            get => _saveBackupCount;
            set { _saveBackupCount = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("save_last_backed_up_at")]
        public DateTime? SaveLastBackedUpAt
        {
            get => _saveLastBackedUpAt;
            set { _saveLastBackedUpAt = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("save_backup_size_mb")]
        public double SaveBackupSizeMb
        {
            get => _saveBackupSizeMb;
            set { _saveBackupSizeMb = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("install_source")]
        public string InstallSource
        {
            get => _installSource;
            set { _installSource = value; OnPropertyChanged(); OnPropertyChanged(nameof(CanVerify)); }
        }

        [JsonPropertyName("lan_install_source_device_name")]
        public string? LanInstallSourceDeviceName
        {
            get => _lanInstallSourceDeviceName;
            set { _lanInstallSourceDeviceName = value; OnPropertyChanged(); }
        }

        [JsonPropertyName("lan_install_source_device_id")]
        public string? LanInstallSourceDeviceId
        {
            get => _lanInstallSourceDeviceId;
            set { _lanInstallSourceDeviceId = value; OnPropertyChanged(); }
        }

        [JsonIgnore]
        public bool CanVerify
        {
            get => _canVerify;
            set { _canVerify = value; OnPropertyChanged(); }
        }

        [JsonIgnore]
        public LanPeer? VerifiedSourcePeer
        {
            get => _verifiedSourcePeer;
            set { _verifiedSourcePeer = value; OnPropertyChanged(); }
        }

        [JsonIgnore]
        public bool HasQuickSfv
        {
            get => _hasQuickSfv;
            set { _hasQuickSfv = value; OnPropertyChanged(); }
        }

        public event PropertyChangedEventHandler? PropertyChanged;
        protected void OnPropertyChanged([CallerMemberName] string? name = null)
            => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
    }

    [JsonSourceGenerationOptions(WriteIndented = true)]
    [JsonSerializable(typeof(List<GameEntry>))]
    [JsonSerializable(typeof(List<string>))]
    internal partial class LibrarySourceGenerationContext : JsonSerializerContext { }

    public class GameLibrary
    {
        private static readonly string AppDataFolder = Config.AppDataFolder;

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
