using System;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public static class RegistryHelper
    {
        private const string AppCompatLayersKey = @"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";

        /// <summary>
        /// Checks whether the RUNASADMIN compatibility flag is set for the specified executable path,
        /// checking both Current User and Local Machine layers.
        /// </summary>
        public static bool GetCompatFlagRunAsAdmin(string exePath)
        {
            if (string.IsNullOrEmpty(exePath)) return false;
            try
            {
                // Check Current User AppCompatFlags
                using (var key = Registry.CurrentUser.OpenSubKey(AppCompatLayersKey))
                {
                    if (key != null)
                    {
                        var value = key.GetValue(exePath) as string;
                        if (value != null && value.Contains("RUNASADMIN", StringComparison.OrdinalIgnoreCase))
                        {
                            return true;
                        }
                    }
                }

                // Check Local Machine AppCompatFlags (often set for all users)
                using (var key = Registry.LocalMachine.OpenSubKey(AppCompatLayersKey))
                {
                    if (key != null)
                    {
                        var value = key.GetValue(exePath) as string;
                        if (value != null && value.Contains("RUNASADMIN", StringComparison.OrdinalIgnoreCase))
                        {
                            return true;
                        }
                    }
                }
            }
            catch (Exception ex)
            {
                App.Log($"RegistryHelper.GetCompatFlagRunAsAdmin failed: {ex.Message}");
            }
            return false;
        }

        /// <summary>
        /// Sets the RUNASADMIN compatibility flag for the specified executable in Current User.
        /// </summary>
        public static void SetCompatFlagRunAsAdmin(string exePath)
        {
            if (string.IsNullOrEmpty(exePath)) return;
            try
            {
                using (var key = Registry.CurrentUser.CreateSubKey(AppCompatLayersKey))
                {
                    if (key != null)
                    {
                        var value = key.GetValue(exePath) as string;
                        if (value == null)
                        {
                            key.SetValue(exePath, "~ RUNASADMIN");
                        }
                        else if (!value.Contains("RUNASADMIN", StringComparison.OrdinalIgnoreCase))
                        {
                            key.SetValue(exePath, value + " RUNASADMIN");
                        }
                    }
                }
            }
            catch (Exception ex)
            {
                App.Log($"RegistryHelper.SetCompatFlagRunAsAdmin failed: {ex.Message}");
            }
        }

        /// <summary>
        /// Removes the RUNASADMIN compatibility flag for the specified executable in Current User.
        /// </summary>
        public static void RemoveCompatFlagRunAsAdmin(string exePath)
        {
            if (string.IsNullOrEmpty(exePath)) return;
            try
            {
                using (var key = Registry.CurrentUser.OpenSubKey(AppCompatLayersKey, writable: true))
                {
                    if (key != null)
                    {
                        var value = key.GetValue(exePath) as string;
                        if (value != null)
                        {
                            if (value.Contains("RUNASADMIN", StringComparison.OrdinalIgnoreCase))
                            {
                                // Remove RUNASADMIN, and clean up the value
                                string newValue = value.Replace("RUNASADMIN", "", StringComparison.OrdinalIgnoreCase)
                                                       .Replace("~", "")
                                                       .Trim();

                                if (string.IsNullOrEmpty(newValue))
                                {
                                    key.DeleteValue(exePath, throwOnMissingValue: false);
                                }
                                else
                                {
                                    key.SetValue(exePath, "~ " + newValue);
                                }
                            }
                        }
                    }
                }
            }
            catch (Exception ex)
            {
                App.Log($"RegistryHelper.RemoveCompatFlagRunAsAdmin failed: {ex.Message}");
            }
        }
    }
}
