using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using Microsoft.Win32;
using VDFParser.Models;

namespace LudusaviWrap
{
    public sealed class SteamUserInfo
    {
        public string UserId { get; init; } = "";
        public string ShortcutsPath { get; init; } = "";
        public DateTime LastModified { get; init; }
    }

    public static class SteamIntegration
    {
        public static string? GetSteamInstallPath()
        {
            // 64-bit Windows: Steam is a 32-bit app, registered under Wow6432Node
            using var key = Registry.LocalMachine.OpenSubKey(@"SOFTWARE\WOW6432Node\Valve\Steam");
            var path = key?.GetValue("InstallPath") as string;
            if (path != null && Directory.Exists(path))
                return path;

            // Fallback for 32-bit hosts
            using var key32 = Registry.LocalMachine.OpenSubKey(@"SOFTWARE\Valve\Steam");
            path = key32?.GetValue("InstallPath") as string;
            if (path != null && Directory.Exists(path))
                return path;

            return null;
        }

        public static List<SteamUserInfo> GetSteamUsers(string steamPath)
        {
            string userdata = Path.Combine(steamPath, "userdata");
            if (!Directory.Exists(userdata))
                return new List<SteamUserInfo>();

            var result = new List<SteamUserInfo>();
            foreach (string dir in Directory.GetDirectories(userdata))
            {
                string shortcutsPath = Path.Combine(dir, "config", "shortcuts.vdf");
                if (File.Exists(shortcutsPath))
                {
                    result.Add(new SteamUserInfo
                    {
                        UserId = Path.GetFileName(dir),
                        ShortcutsPath = shortcutsPath,
                        LastModified = File.GetLastWriteTime(shortcutsPath)
                    });
                }
            }
            return result;
        }

        // CRC32 of ("\"" + exePath + "\"" + gameName) with top bit set — matches Steam's internal formula
        public static uint CalculateAppId(string exePath, string gameName)
        {
            string input = "\"" + exePath + "\"" + gameName;
            byte[] bytes = Encoding.UTF8.GetBytes(input);
            uint crc = ComputeCrc32(bytes);
            return crc | 0x80000000;
        }

        public static VDFEntry[] ReadShortcuts(string vdfPath)
        {
            if (!File.Exists(vdfPath))
                return Array.Empty<VDFEntry>();

            try
            {
                return VDFParser.VDFParser.Parse(vdfPath);
            }
            catch
            {
                // Re-throw so callers can handle corrupted files
                throw;
            }
        }

        // Returns true if a new entry was added, false if an existing one was updated
        public static bool UpsertShortcut(
            ref VDFEntry[] entries,
            string gameName,
            string launcherExePath,
            string startDir)
        {
            string quotedExe = "\"" + launcherExePath + "\"";
            string quotedDir = "\"" + startDir + "\"";

            for (int i = 0; i < entries.Length; i++)
            {
                string? existingExe = entries[i].Exe?.Trim('"');
                if (string.Equals(existingExe, launcherExePath, StringComparison.OrdinalIgnoreCase))
                {
                    entries[i].AppName = gameName;
                    entries[i].StartDir = quotedDir;
                    entries[i].Icon = launcherExePath;
                    return false;
                }
            }

            uint appId = CalculateAppId(launcherExePath, gameName);
            var newEntry = new VDFEntry
            {
                Index = entries.Length,
                appid = (int)appId,
                AppName = gameName,
                Exe = quotedExe,
                StartDir = quotedDir,
                Icon = launcherExePath,
                ShortcutPath = "",
                LaunchOptions = "",
                IsHidden = 0,
                AllowDesktopConfig = 1,
                AllowOverlay = 1,
                OpenVR = 0,
                Devkit = 0,
                DevkitGameID = "",
                LastPlayTime = 0,
                Tags = new[] { "Spool" }
            };

            Array.Resize(ref entries, entries.Length + 1);
            entries[entries.Length - 1] = newEntry;
            return true;
        }

        public static void WriteShortcuts(string vdfPath, VDFEntry[] entries)
        {
            // Backup before writing
            if (File.Exists(vdfPath))
                File.Copy(vdfPath, vdfPath + ".bak", overwrite: true);

            byte[] output = VDFParser.VDFSerializer.Serialize(entries);

            // Atomic write
            string tmp = vdfPath + ".tmp";
            File.WriteAllBytes(tmp, output);
            File.Move(tmp, vdfPath, overwrite: true);
        }

        // suffix: "" for horizontal grid, "p" for portrait, "_hero" for hero, "_logo" for logo
        public static string? CopyGridImage(string? sourcePath, string steamPath, string userId, uint appId, string suffix = "")
        {
            if (sourcePath == null || !File.Exists(sourcePath))
                return null;

            string ext = Path.GetExtension(sourcePath);
            string gridDir = Path.Combine(steamPath, "userdata", userId, "config", "grid");
            Directory.CreateDirectory(gridDir);

            string destPath = Path.Combine(gridDir, $"{appId}{suffix}{ext}");
            File.Copy(sourcePath, destPath, overwrite: true);
            return destPath;
        }

        public static bool IsSteamRunning() =>
            Process.GetProcessesByName("steam").Length > 0;

        // Standard CRC32 (ISO 3309 / ITU-T V.42 polynomial)
        private static readonly uint[] Crc32Table = BuildCrc32Table();

        private static uint[] BuildCrc32Table()
        {
            const uint poly = 0xEDB88320;
            var table = new uint[256];
            for (uint i = 0; i < 256; i++)
            {
                uint c = i;
                for (int j = 0; j < 8; j++)
                    c = (c & 1) != 0 ? poly ^ (c >> 1) : c >> 1;
                table[i] = c;
            }
            return table;
        }

        private static uint ComputeCrc32(byte[] data)
        {
            uint crc = 0xFFFFFFFF;
            foreach (byte b in data)
                crc = Crc32Table[(crc ^ b) & 0xFF] ^ (crc >> 8);
            return crc ^ 0xFFFFFFFF;
        }
    }
}
