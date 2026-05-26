using System;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Input;
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

            var theme = useDark ? ApplicationTheme.Dark : ApplicationTheme.Light;
            ApplicationThemeManager.Apply(theme, updateAccent: true);

            foreach (System.Windows.Window window in System.Windows.Application.Current.Windows)
                WindowBackgroundManager.UpdateBackground(window, theme, Wpf.Ui.Controls.WindowBackdropType.Mica);
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

    public partial class App : Application, System.ComponentModel.INotifyPropertyChanged
    {
        private bool _isTouchOptimized;

        public bool IsTouchOptimized
        {
            get => _isTouchOptimized;
            set
            {
                if (_isTouchOptimized != value)
                {
                    _isTouchOptimized = value;
                    OnPropertyChanged(nameof(IsTouchOptimized));
                }
            }
        }

        public event System.ComponentModel.PropertyChangedEventHandler? PropertyChanged;

        protected virtual void OnPropertyChanged(string propertyName)
        {
            PropertyChanged?.Invoke(this, new System.ComponentModel.PropertyChangedEventArgs(propertyName));
        }

        public App()
        {
            AppContext.SetSwitch("Switch.System.Windows.Input.Stylus.EnablePointerSupport", true);

            EventManager.RegisterClassHandler(typeof(System.Windows.Controls.TextBox),
                System.Windows.Controls.TextBox.PreviewTouchDownEvent,
                new EventHandler<TouchEventArgs>(TextBox_PreviewTouchDown));
            EventManager.RegisterClassHandler(typeof(System.Windows.Controls.PasswordBox),
                System.Windows.Controls.PasswordBox.PreviewTouchDownEvent,
                new EventHandler<TouchEventArgs>(PasswordBox_PreviewTouchDown));
        }

        private static void TextBox_PreviewTouchDown(object? sender, TouchEventArgs e)
        {
            if (Application.Current is App app && app.IsTouchOptimized)
            {
                ShowTouchKeyboard();
            }
        }

        private static void PasswordBox_PreviewTouchDown(object? sender, TouchEventArgs e)
        {
            if (Application.Current is App app && app.IsTouchOptimized)
            {
                ShowTouchKeyboard();
            }
        }

        public static void ShowTouchKeyboard()
        {
            try
            {
                string path = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.CommonProgramFiles),
                    @"Microsoft Shared\ink\TabTip.exe"
                );
                if (File.Exists(path))
                {
                    Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
                }
            }
            catch { }
        }

        private static readonly string LogPath = Path.Combine(Config.AppDataFolder, "debug.log");

        internal static void Log(string message)
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(LogPath)!);
                File.AppendAllText(LogPath, $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff}] {message}{Environment.NewLine}");
            }
            catch { }
        }

        private static void MigrateAppData()
        {
            string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string oldDir = Path.Combine(localAppData, "ludusavi-wrap");
            string newDir = Config.AppDataFolder;

            if (Directory.Exists(oldDir) && !Directory.Exists(newDir))
            {
                try { Directory.Move(oldDir, newDir); }
                catch { }
            }
        }

        protected override void OnStartup(StartupEventArgs e)
        {
            MigrateAppData();

            AppDomain.CurrentDomain.UnhandledException += (s, ex) =>
                Log($"FATAL UnhandledException: {ex.ExceptionObject}");
            DispatcherUnhandledException += (s, ex) =>
            {
                Log($"FATAL DispatcherUnhandledException: {ex.Exception}");
                ex.Handled = true;
                MessageBox.Show($"Unexpected error:\n{ex.Exception.Message}\n\nSee debug.log in %LOCALAPPDATA%\\Spool",
                                "Spool Error", MessageBoxButton.OK, MessageBoxImage.Error);
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

                    ShutdownMode = ShutdownMode.OnExplicitShutdown;
                    Dispatcher.InvokeAsync(async () =>
                    {
                        try
                        {
                            await new RunWorkflow(gameName, gameExe).ExecuteAsync();
                        }
                        catch (Exception ex)
                        {
                            Log($"RunWorkflow unexpected error for '{gameName}': {ex}");
                            MessageBox.Show($"An unexpected error occurred: {ex.Message}",
                                "Spool Error", MessageBoxButton.OK, MessageBoxImage.Error);
                        }
                        finally
                        {
                            Shutdown();
                        }
                    });
                }
                else
                {
                    Log("--run mode: insufficient arguments");
                    MessageBox.Show("Invalid command-line arguments. Usage: --run <GameName> <GameExe>",
                                    "Spool Error", MessageBoxButton.OK, MessageBoxImage.Error);
                    Shutdown();
                }
                return;
            }

            Log("Loading config...");
            var config = new Config();
            IsTouchOptimized = config.Data.TouchOptimized;
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
                var library = new GameLibrary();
                var mainWindow = new MainWindow(config, library);
                Log("Calling MainWindow.Show()...");
                mainWindow.Show();
                Log("MainWindow.Show() returned");
            }
            catch (Exception ex)
            {
                Log($"EXCEPTION creating/showing MainWindow: {ex}");
                MessageBox.Show($"Failed to open main window:\n{ex.Message}\n\nSee debug.log in %LOCALAPPDATA%\\Spool",
                                "Spool Error", MessageBoxButton.OK, MessageBoxImage.Error);
                Shutdown();
            }
        }

        private static void ApplyThemeFromConfig()
        {
            try
            {
                var config = new Config();
                ThemeManager.ApplyTheme(config.Data.Theme);
                if (Application.Current is App app)
                {
                    app.IsTouchOptimized = config.Data.TouchOptimized;
                }
            }
            catch
            {
                ThemeManager.ApplyTheme("system");
            }
        }
    }
}
