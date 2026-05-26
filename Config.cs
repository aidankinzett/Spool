using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace LudusaviWrap
{
    public class ConfigData
    {
        [JsonPropertyName("ludusavi_path")]
        public string LudusaviPath { get; set; } = "";

        [JsonPropertyName("steamgriddb_enabled")]
        public bool SteamGridDbEnabled { get; set; } = false;

        [JsonPropertyName("steamgriddb_api_key")]
        public string SteamGridDbApiKey { get; set; } = "";

        [JsonPropertyName("spool_exe")]
        public string SpoolExe { get; set; } = "";

        [JsonPropertyName("theme")]
        public string Theme { get; set; } = "system";

        [JsonPropertyName("sync_server_enabled")]
        public bool SyncServerEnabled { get; set; } = false;

        [JsonPropertyName("sync_server_url")]
        public string SyncServerUrl { get; set; } = "";

        [JsonPropertyName("sync_server_api_key")]
        public string SyncServerApiKey { get; set; } = "";

        [JsonPropertyName("device_id")]
        public string DeviceId { get; set; } = "";

        [JsonPropertyName("device_name")]
        public string DeviceName { get; set; } = "";

        [JsonPropertyName("lan_share_enabled")]
        public bool LanShareEnabled { get; set; } = true;

        [JsonPropertyName("lan_share_port")]
        public int LanSharePort { get; set; } = 47632;

        [JsonPropertyName("lan_install_dir")]
        public string LanInstallDir { get; set; } = "";

        [JsonPropertyName("torbox_enabled")]
        public bool TorBoxEnabled { get; set; } = false;

        [JsonPropertyName("torbox_api_key")]
        public string TorBoxApiKey { get; set; } = "";

        [JsonPropertyName("download_dir")]
        public string DownloadDir { get; set; } = "";

        [JsonPropertyName("download_sources")]
        public List<string> DownloadSources { get; set; } = new();

        [JsonIgnore]
        [JsonPropertyName("touch_optimized")]
        public bool TouchOptimized { get; set; } = false;

        [JsonPropertyName("touch_mode")]
        public string TouchMode { get; set; } = "auto";
    }

    [JsonSourceGenerationOptions(WriteIndented = true)]
    [JsonSerializable(typeof(ConfigData))]
    [JsonSerializable(typeof(List<string>))]
    internal partial class ConfigSourceGenerationContext : JsonSerializerContext
    {
    }

    public class Config
    {
        public static readonly string AppDataFolder = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Spool"
        );

        private static readonly string ConfigPath = Path.Combine(AppDataFolder, "config.json");

        public ConfigData Data { get; private set; }

        public bool TouchscreenDetected { get; private set; }

        public bool IsEffectivelyTouchOptimized => Data.TouchMode switch
        {
            "on"  => true,
            "off" => false,
            _     => TouchscreenDetected,
        };

        public Config()
        {
            Data = new ConfigData();
            TouchscreenDetected = IsTouchscreenConnected();
            Load();
            EnsureDeviceIdentity();
            AutoDetectLudusavi();
            SaveCurrentExePath();
        }

        private static bool IsTouchscreenConnected()
        {
            try
            {
                var devices = System.Windows.Input.Tablet.TabletDevices;
                App.Log($"[Config] Touch detection: {devices.Count} tablet device(s) enumerated");
                foreach (System.Windows.Input.TabletDevice device in devices)
                {
                    App.Log($"[Config] Touch detection: device '{device.Name}' type={device.Type}");
                    if (device.Type == System.Windows.Input.TabletDeviceType.Touch)
                    {
                        App.Log("[Config] Touch detection: touchscreen found — enabling touch-optimized mode");
                        return true;
                    }
                }
                App.Log("[Config] Touch detection: no touchscreen found");
            }
            catch (Exception ex)
            {
                App.Log($"[Config] Touch detection failed: {ex.Message}");
            }
            return false;
        }

        private void Load()
        {
            try
            {
                if (File.Exists(ConfigPath))
                {
                    string json = File.ReadAllText(ConfigPath);
                    var loaded = JsonSerializer.Deserialize(json, ConfigSourceGenerationContext.Default.ConfigData);
                    if (loaded != null)
                    {
                        // Migrate from old bool to three-state TouchMode
                        if (!json.Contains("\"touch_mode\""))
                        {
                            if (json.Contains("\"touch_optimized\""))
                            {
                                // Old config: parse value from raw JSON since [JsonIgnore] prevents deserialization
                                bool oldVal = json.Contains("\"touch_optimized\": true") ||
                                              json.Contains("\"touch_optimized\":true");
                                loaded.TouchMode = oldVal ? "on" : "auto";
                                App.Log($"[Config] Migrated touch_optimized={oldVal} → touch_mode={loaded.TouchMode}");
                            }
                            else
                            {
                                loaded.TouchMode = "auto";
                                App.Log("[Config] No touch setting found — defaulting to auto");
                            }
                        }
                        Data = loaded;
                    }
                }
                else
                {
                    App.Log("[Config] No config file found — defaulting touch_mode=auto");
                    Data.TouchMode = "auto";
                }
            }
            catch
            {
                // Ignore load failures and proceed with defaults
            }
        }

        public void Save()
        {
            try
            {
                Directory.CreateDirectory(AppDataFolder);
                string json = JsonSerializer.Serialize(Data, ConfigSourceGenerationContext.Default.ConfigData);
                File.WriteAllText(ConfigPath, json);
            }
            catch
            {
                // Ignore save failures
            }
        }

        private void EnsureDeviceIdentity()
        {
            bool changed = false;
            if (string.IsNullOrEmpty(Data.DeviceId))   { Data.DeviceId   = Guid.NewGuid().ToString(); changed = true; }
            if (string.IsNullOrEmpty(Data.DeviceName)) { Data.DeviceName = Environment.MachineName;   changed = true; }
            if (changed) Save();
        }

        private void AutoDetectLudusavi()
        {
            if (!string.IsNullOrEmpty(Data.LudusaviPath) && File.Exists(Data.LudusaviPath))
            {
                return;
            }

            // Check current directory
            string localPath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "ludusavi.exe");
            if (File.Exists(localPath))
            {
                Data.LudusaviPath = Path.GetFullPath(localPath);
                Save();
                return;
            }

            // Check System PATH
            string? pathEnv = Environment.GetEnvironmentVariable("PATH");
            if (!string.IsNullOrEmpty(pathEnv))
            {
                string[] paths = pathEnv.Split(Path.PathSeparator);
                foreach (string path in paths)
                {
                    try
                    {
                        string fullPath = Path.Combine(path, "ludusavi.exe");
                        if (File.Exists(fullPath))
                        {
                            Data.LudusaviPath = Path.GetFullPath(fullPath);
                            Save();
                            return;
                        }
                    }
                    catch
                    {
                        // Ignore invalid characters in PATH entries
                    }
                }
            }
        }

        private void SaveCurrentExePath()
        {
            try
            {
                string exePath = Environment.ProcessPath ?? "";
                if (!string.IsNullOrEmpty(exePath))
                {
                    Data.SpoolExe = exePath;
                    Save();
                }
            }
            catch
            {
                // Ignore errors
            }
        }

        public bool IsLudusaviOk => !string.IsNullOrEmpty(Data.LudusaviPath) && File.Exists(Data.LudusaviPath);

        public bool IsTorBoxOk => Data.TorBoxEnabled && !string.IsNullOrEmpty(Data.TorBoxApiKey);

        public string EffectiveDownloadDir =>
            !string.IsNullOrEmpty(Data.DownloadDir)
                ? Data.DownloadDir
                : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads");
    }
}
