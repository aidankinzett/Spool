using System;
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

        [JsonPropertyName("ludusavi_wrap_exe")]
        public string LudusaviWrapExe { get; set; } = "";

        [JsonPropertyName("theme")]
        public string Theme { get; set; } = "system";
    }

    [JsonSourceGenerationOptions(WriteIndented = true)]
    [JsonSerializable(typeof(ConfigData))]
    internal partial class ConfigSourceGenerationContext : JsonSerializerContext
    {
    }

    public class Config
    {
        private static readonly string AppDataFolder = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ludusavi-wrap"
        );

        private static readonly string ConfigPath = Path.Combine(AppDataFolder, "config.json");

        public ConfigData Data { get; private set; }

        public Config()
        {
            Data = new ConfigData();
            Load();
            AutoDetectLudusavi();
            SaveCurrentExePath();
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
                        Data = loaded;
                    }
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
                    Data.LudusaviWrapExe = exePath;
                    Save();
                }
            }
            catch
            {
                // Ignore errors
            }
        }

        public bool IsLudusaviOk => !string.IsNullOrEmpty(Data.LudusaviPath) && File.Exists(Data.LudusaviPath);
    }
}
