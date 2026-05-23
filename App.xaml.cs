using System;
using System.IO;
using System.Windows;
using System.Windows.Media;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public static class ThemeManager
    {
        private const string LightSource = "Theme.Light.xaml";
        private const string DarkSource  = "Theme.Dark.xaml";

        /// <summary>
        /// Resolves the effective theme from the user preference string
        /// ("system" | "light" | "dark") and swaps the colour palette in
        /// Application.Current.Resources.
        /// </summary>
        public static void ApplyTheme(string preference)
        {
            bool useDark = preference switch
            {
                "dark"  => true,
                "light" => false,
                _       => IsSystemDark()
            };

            string source = useDark ? DarkSource : LightSource;

            var dict = new ResourceDictionary
            {
                Source = new Uri($"pack://application:,,,/{source}", UriKind.Absolute)
            };

            // Remove any previously-loaded palette dictionary (Light or Dark)
            var merged = Application.Current.Resources.MergedDictionaries;
            for (int i = merged.Count - 1; i >= 0; i--)
            {
                string? src = merged[i].Source?.OriginalString;
                if (src != null && (src.EndsWith(LightSource) || src.EndsWith(DarkSource)))
                {
                    merged.RemoveAt(i);
                }
            }

            merged.Add(dict);
        }

        private static bool IsSystemDark()
        {
            try
            {
                using var key = Registry.CurrentUser.OpenSubKey(
                    @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
                if (key?.GetValue("AppsUseLightTheme") is int value)
                    return value == 0;
            }
            catch { }
            return false;
        }
    }

    public partial class App : Application
    {
        private static readonly string LogPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ludusavi-wrap", "debug.log");

        internal static void Log(string message)
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(LogPath)!);
                File.AppendAllText(LogPath, $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff}] {message}{Environment.NewLine}");
            }
            catch { }
        }

        protected override void OnStartup(StartupEventArgs e)
        {
            // Catch anything that slips past individual try/catches
            AppDomain.CurrentDomain.UnhandledException += (s, ex) =>
                Log($"FATAL UnhandledException: {ex.ExceptionObject}");
            DispatcherUnhandledException += (s, ex) =>
            {
                Log($"FATAL DispatcherUnhandledException: {ex.Exception}");
                ex.Handled = true;
                MessageBox.Show($"Unexpected error:\n{ex.Exception.Message}\n\nSee debug.log in %LOCALAPPDATA%\\ludusavi-wrap",
                                "Ludusavi Wrap Error", MessageBoxButton.OK, MessageBoxImage.Error);
                Shutdown();
            };

            Log("=== App starting ===");
            Log($"Args: [{string.Join(", ", e.Args)}]");

            base.OnStartup(e);

            string[] args = e.Args;

            if (args.Length > 0 && args[0] == "--run")
            {
                if (args.Length >= 3)
                {
                    // Apply theme from config before showing the window
                    ApplyThemeFromConfig();
                    ApplySystemAccentColor();

                    string gameName = args[1];
                    string gameExe  = args[2];
                    Log($"--run mode: game='{gameName}' exe='{gameExe}'");
                    var runWindow = new RunWindow(gameName, gameExe);
                    runWindow.Show();
                }
                else
                {
                    Log("--run mode: insufficient arguments");
                    MessageBox.Show("Invalid command-line arguments. Usage: --run <GameName> <GameExe>",
                                    "Ludusavi Wrap Error", MessageBoxButton.OK, MessageBoxImage.Error);
                    Shutdown();
                }
                return;
            }

            Log("Loading config...");
            var config = new Config();
            Log($"LudusaviPath = '{config.Data.LudusaviPath}'");
            Log($"IsLudusaviOk = {config.IsLudusaviOk}");
            Log($"Theme = '{config.Data.Theme}'");

            ThemeManager.ApplyTheme(config.Data.Theme);
            ApplySystemAccentColor();

            if (!config.IsLudusaviOk)
            {
                Log("Showing SetupWindow (ludusavi not found)");
                var setupWindow = new SetupWindow(config, isFirstRun: true);
                bool? result = setupWindow.ShowDialog();
                Log($"SetupWindow result = {result}");
                if (result != true)
                {
                    Log("Setup cancelled or closed — shutting down");
                    Shutdown();
                    return;
                }
                Log($"Setup saved. LudusaviPath is now '{config.Data.LudusaviPath}'");
            }

            Log("Creating MainWindow...");
            try
            {
                var mainWindow = new MainWindow(config);
                Log("Calling MainWindow.Show()...");
                mainWindow.Show();
                Log("MainWindow.Show() returned");
            }
            catch (Exception ex)
            {
                Log($"EXCEPTION creating/showing MainWindow: {ex}");
                MessageBox.Show($"Failed to open main window:\n{ex.Message}\n\nSee debug.log in %LOCALAPPDATA%\\ludusavi-wrap",
                                "Ludusavi Wrap Error", MessageBoxButton.OK, MessageBoxImage.Error);
                Shutdown();
            }
        }

        /// <summary>
        /// Reads the theme preference from the config file on disk and applies it.
        /// Used by --run mode which needs the theme before a Config object is built for the UI.
        /// </summary>
        private static void ApplyThemeFromConfig()
        {
            try
            {
                var config = new Config();
                ThemeManager.ApplyTheme(config.Data.Theme);
            }
            catch
            {
                ThemeManager.ApplyTheme("system");
            }
        }

        private void ApplySystemAccentColor()
        {
            try
            {
                using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\DWM");
                if (key?.GetValue("AccentColor") is int abgr)
                {
                    byte r = (byte)(abgr & 0xFF);
                    byte g = (byte)((abgr >> 8) & 0xFF);
                    byte b = (byte)((abgr >> 16) & 0xFF);
                    var brush = new SolidColorBrush(Color.FromRgb(r, g, b));
                    brush.Freeze();
                    Resources["AccentBrush"] = brush;
                    Log($"Accent color applied: #{r:X2}{g:X2}{b:X2}");
                }
            }
            catch (Exception ex)
            {
                Log($"ApplySystemAccentColor failed (non-fatal): {ex.Message}");
            }
        }
    }
}
