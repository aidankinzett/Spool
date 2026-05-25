using System;
using System.Text.RegularExpressions;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public static class RegistryHelper
    {
        private const string AppCompatLayersKey = @"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";

        public static bool GetCompatFlagRunAsAdmin(string exePath)
        {
            if (string.IsNullOrEmpty(exePath)) return false;
            try
            {
                using var cuKey = Registry.CurrentUser.OpenSubKey(AppCompatLayersKey);
                if (cuKey?.GetValue(exePath) is string cuVal &&
                    cuVal.Contains("RUNASADMIN", StringComparison.OrdinalIgnoreCase))
                    return true;

                // Also check HKLM — some games are flagged for all users
                using var lmKey = Registry.LocalMachine.OpenSubKey(AppCompatLayersKey);
                if (lmKey?.GetValue(exePath) is string lmVal &&
                    lmVal.Contains("RUNASADMIN", StringComparison.OrdinalIgnoreCase))
                    return true;
            }
            catch (Exception ex)
            {
                App.Log($"RegistryHelper.GetCompatFlagRunAsAdmin failed: {ex.Message}");
            }
            return false;
        }

        public static void SetCompatFlagRunAsAdmin(string exePath)
        {
            if (string.IsNullOrEmpty(exePath)) return;
            try
            {
                using var key = Registry.CurrentUser.CreateSubKey(AppCompatLayersKey);
                if (key == null) return;
                var value = key.GetValue(exePath) as string;
                if (value == null)
                    key.SetValue(exePath, "~ RUNASADMIN");
                else if (!value.Contains("RUNASADMIN", StringComparison.OrdinalIgnoreCase))
                    key.SetValue(exePath, value + " RUNASADMIN");
            }
            catch (Exception ex)
            {
                App.Log($"RegistryHelper.SetCompatFlagRunAsAdmin failed: {ex.Message}");
            }
        }

        public static void RemoveCompatFlagRunAsAdmin(string exePath)
        {
            if (string.IsNullOrEmpty(exePath)) return;
            try
            {
                using var key = Registry.CurrentUser.OpenSubKey(AppCompatLayersKey, writable: true);
                if (key == null) return;
                var value = key.GetValue(exePath) as string;
                if (value == null || !value.Contains("RUNASADMIN", StringComparison.OrdinalIgnoreCase))
                    return;

                // Remove the RUNASADMIN token only, preserving other flags and the leading ~ marker
                string stripped = Regex.Replace(value, @"\bRUNASADMIN\b", "", RegexOptions.IgnoreCase).Trim();

                // Collapse any runs of whitespace left by the removal
                stripped = Regex.Replace(stripped, @"\s+", " ").Trim();

                if (string.IsNullOrEmpty(stripped) || stripped == "~")
                {
                    key.DeleteValue(exePath, throwOnMissingValue: false);
                }
                else
                {
                    // Ensure the ~ marker is present for the remaining flags
                    if (!stripped.StartsWith("~"))
                        stripped = "~ " + stripped;
                    key.SetValue(exePath, stripped);
                }
            }
            catch (Exception ex)
            {
                App.Log($"RegistryHelper.RemoveCompatFlagRunAsAdmin failed: {ex.Message}");
            }
        }
    }
}
