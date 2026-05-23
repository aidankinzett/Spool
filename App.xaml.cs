using System;
using System.IO;
using System.Windows;
using Microsoft.Win32;
using Wpf.Ui.Appearance;

namespace LudusaviWrap
{
    public static class ThemeManager
    {
        public static void ApplyTheme(string preference)
        {
            bool useDark = preference switch
            {
                "dark"  => true,
                "light" => false,
                _       => IsSystemDark()
            };

            ApplicationThemeManager.Apply(
                useDark ? ApplicationTheme.Dark : ApplicationTheme.Light,
                updateAccent: true
            );
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
                    ApplyThemeFromConfig();

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
    }
}
