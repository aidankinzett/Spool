using System;
using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace LudusaviWrap
{
    public class LudusaviFindResponse
    {
        [JsonPropertyName("games")]
        public System.Collections.Generic.Dictionary<string, object>? Games { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = true)]
    [JsonSerializable(typeof(LudusaviFindResponse))]
    internal partial class MainSourceGenerationContext : JsonSerializerContext { }

    public class SyncStatusToBrushConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            return (SyncStatus)value switch
            {
                SyncStatus.Synced          => new SolidColorBrush(Color.FromRgb(0x4C, 0xAF, 0x50)),
                SyncStatus.LocalNotSynced  => new SolidColorBrush(Color.FromRgb(0xFF, 0x98, 0x00)),
                SyncStatus.CloudNotSynced  => new SolidColorBrush(Color.FromRgb(0x21, 0x96, 0xF3)),
                _                          => new SolidColorBrush(Colors.Transparent)
            };
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class SyncStatusToLabelConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            return (SyncStatus)value switch
            {
                SyncStatus.Synced          => "Synced",
                SyncStatus.LocalNotSynced  => "Local not synced",
                SyncStatus.CloudNotSynced  => "Cloud not synced",
                _                          => ""
            };
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class SyncStatusToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => (SyncStatus)value == SyncStatus.Unknown ? Visibility.Collapsed : Visibility.Visible;
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class StringToImageConverter : IValueConverter
    {
        public object? Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string path || string.IsNullOrEmpty(path) || !File.Exists(path))
                return null;
            try
            {
                var bi = new BitmapImage();
                bi.BeginInit();
                bi.UriSource = new Uri(path);
                bi.CacheOption = BitmapCacheOption.OnLoad;
                bi.EndInit();
                bi.Freeze();
                return bi;
            }
            catch { return null; }
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public partial class MainWindow : Wpf.Ui.Controls.FluentWindow
    {
        public static readonly string Version =
            System.Reflection.Assembly.GetEntryAssembly()?.GetName().Version?.ToString(3) ?? "0.0.0";

        private readonly Config _config;
        private readonly GameLibrary _library;
        private readonly ObservableCollection<GameEntry> _games = new();
        private SuccessWindow? _successWindow;
        private bool _updateCheckSubscribed = false;
        private readonly LanShareServer _lanServer;

        public MainWindow(Config config, GameLibrary library)
        {
            InitializeComponent();
            _config = config;
            _library = library;
            Title = $"Ludusavi Wrap v{Version}";

            _lanServer = new LanShareServer(config.Data.DeviceName, config.Data.DeviceId);

            GamesGrid.ItemsSource = _games;
            LoadGames();
            Loaded  += MainWindow_Loaded;
            Closing += MainWindow_Closing;
        }

        private void LoadGames()
        {
            _games.Clear();
            foreach (var entry in _library.Entries)
                _games.Add(entry);
            UpdateEmptyState();
        }

        private void UpdateEmptyState()
        {
            bool empty = _games.Count == 0;
            EmptyState.Visibility = empty ? Visibility.Visible : Visibility.Collapsed;
            LibraryScrollViewer.Visibility = empty ? Visibility.Collapsed : Visibility.Visible;
        }

        private void MainWindow_Closing(object? sender, System.ComponentModel.CancelEventArgs e)
        {
            _lanServer.Stop();
        }

        private void StartLanServerIfEnabled()
        {
            if (!_config.Data.LanShareEnabled) return;
            try
            {
                _lanServer.Start(() => _library.Entries, _config.Data.LanSharePort);
            }
            catch (Exception ex)
            {
                App.Log($"LAN server failed to start: {ex.Message}");
            }
        }

        private void MainWindow_Loaded(object sender, RoutedEventArgs e)
        {
            StartLanServerIfEnabled();

            if (System.Version.TryParse(Version, out var parsedVersion))
                AutoUpdaterDotNET.AutoUpdater.InstalledVersion = parsedVersion;

            AutoUpdaterDotNET.AutoUpdater.ShowSkipButton = false;
            AutoUpdaterDotNET.AutoUpdater.ShowRemindLaterButton = false;
            AutoUpdaterDotNET.AutoUpdater.SetOwner(this);

            if (!_updateCheckSubscribed)
            {
                _updateCheckSubscribed = true;
                AutoUpdaterDotNET.AutoUpdater.CheckForUpdateEvent += (args) =>
                {
                    if (args.Error != null) return;
                    if (args.IsUpdateAvailable)
                        AutoUpdaterDotNET.AutoUpdater.ShowUpdateForm(args);
                };
            }

            AutoUpdaterDotNET.AutoUpdater.Start(
                "https://raw.githubusercontent.com/aidankinzett/ludusavi-wrap/master/update.xml");
        }

        private void Settings_Click(object sender, RoutedEventArgs e)
        {
            var setup = new SetupWindow(_config);
            setup.Owner = this;
            setup.ShowDialog();
        }

        private void AddGame_Click(object sender, RoutedEventArgs e)
        {
            var dlg = new AddGameWindow(_config, _library);
            dlg.Owner = this;
            dlg.OnCoverArtFetched = (entry, _) => UpdateEmptyState();
            if (dlg.ShowDialog() == true)
            {
                // Sync newly added/updated entries from the library into the observable collection
                foreach (var entry in _library.Entries)
                {
                    if (!_games.Any(g => g.Id == entry.Id))
                        _games.Add(entry);
                }
                UpdateEmptyState();
            }
        }

        private void Play_Click(object sender, RoutedEventArgs e)
        {
            if ((sender as Button)?.DataContext is not GameEntry entry) return;

            if (!File.Exists(entry.ExePath))
            {
                MessageBox.Show($"Game executable not found:\n{entry.ExePath}",
                    "Game Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            entry.LastPlayedAt = DateTime.UtcNow;
            _library.Update(entry);

            var runWindow = new RunWindow(entry.GameName, entry.ExePath, exitAppOnFinish: false, entry: entry, library: _library);
            runWindow.Owner = this;
            Hide();
            runWindow.Show();
        }

        private GameEntry? GetEntryFromContextMenu(object sender)
        {
            if (sender is MenuItem mi && mi.Parent is ContextMenu cm &&
                cm.PlacementTarget is FrameworkElement fe && fe.DataContext is GameEntry entry)
                return entry;
            return null;
        }

        private async void ContextGenerateAC_Click(object sender, RoutedEventArgs e)
        {
            var entry = GetEntryFromContextMenu(sender);
            if (entry == null) return;

            if (!_config.IsLudusaviOk)
            {
                MessageBox.Show("Ludusavi not found - open Settings to configure it.",
                    "Ludusavi Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            try
            {
                string launcherExePath = await LauncherGenerator.GenerateLauncherExeAsync(entry, _config);
                entry.LauncherExePath = launcherExePath;
                _library.Update(entry);

                _successWindow = new SuccessWindow(this, entry.GameName, launcherExePath, SuccessMode.ArmouryCrate);
                _successWindow.ShowDialog();
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to generate launcher: {ex.Message}",
                    "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private async void ContextAddToSteam_Click(object sender, RoutedEventArgs e)
        {
            var entry = GetEntryFromContextMenu(sender);
            if (entry == null) return;

            if (!_config.IsLudusaviOk)
            {
                MessageBox.Show("Ludusavi not found - open Settings to configure it.",
                    "Ludusavi Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            string launcherExePath;
            try
            {
                launcherExePath = await LauncherGenerator.GenerateLauncherExeAsync(entry, _config);
                entry.LauncherExePath = launcherExePath;
                _library.Update(entry);
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to generate launcher: {ex.Message}",
                    "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            string? steamPath = await Task.Run(() => SteamIntegration.GetSteamInstallPath());
            if (steamPath == null)
            {
                MessageBox.Show("Steam installation not found. Is Steam installed?",
                    "Steam Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            var users = await Task.Run(() => SteamIntegration.GetSteamUsers(steamPath));
            if (users.Count == 0)
            {
                MessageBox.Show("No Steam user profiles found. Launch Steam at least once to create your profile.",
                    "No Steam Users", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            var targetUser = users.OrderByDescending(u => u.LastModified).First();

            if (SteamIntegration.IsSteamRunning())
            {
                var answer = MessageBox.Show(
                    "Steam is currently running. Writing to shortcuts.vdf while Steam is open may cause your changes to be overwritten when Steam exits.\n\nClose Steam first, or the shortcut may not appear.\n\nContinue anyway?",
                    "Steam Is Running", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                if (answer == MessageBoxResult.No) return;
            }

            VDFParser.Models.VDFEntry[] entries;
            try
            {
                entries = await Task.Run(() => SteamIntegration.ReadShortcuts(targetUser.ShortcutsPath));
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to read shortcuts.vdf: {ex.Message}",
                    "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            string startDir = Path.GetDirectoryName(launcherExePath) ?? "";
            SteamIntegration.UpsertShortcut(ref entries, entry.GameName, launcherExePath, startDir);

            try
            {
                await Task.Run(() => SteamIntegration.WriteShortcuts(targetUser.ShortcutsPath, entries));
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to write shortcuts.vdf: {ex.Message}",
                    "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            _successWindow = new SuccessWindow(this, entry.GameName, launcherExePath, SuccessMode.Steam);
            if (users.Count > 1)
                _successWindow.UpdateArtwork($"Added to Steam user {targetUser.UserId}", "#4CAF50");
            _successWindow.ShowDialog();
        }

        private void ContextOpenFolder_Click(object sender, RoutedEventArgs e)
        {
            var entry = GetEntryFromContextMenu(sender);
            if (entry == null) return;

            try
            {
                string? dir = Path.GetDirectoryName(entry.ExePath);
                if (dir != null && Directory.Exists(dir))
                    Process.Start("explorer.exe", $"\"{dir}\"");
                else
                    MessageBox.Show("Game folder not found.", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Could not open folder: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private void ContextRemove_Click(object sender, RoutedEventArgs e)
        {
            var entry = GetEntryFromContextMenu(sender);
            if (entry == null) return;

            var ans = MessageBox.Show($"Remove '{entry.GameName}' from your library?",
                "Remove Game", MessageBoxButton.YesNo, MessageBoxImage.Question);
            if (ans != MessageBoxResult.Yes) return;

            _library.Remove(entry.Id);
            var toRemove = _games.FirstOrDefault(g => g.Id == entry.Id);
            if (toRemove != null) _games.Remove(toRemove);
            _lanServer.InvalidateManifestCache();
            UpdateEmptyState();
        }

        private void ContextSetFolder_Click(object sender, RoutedEventArgs e)
        {
            var entry = GetEntryFromContextMenu(sender);
            if (entry == null) return;

            var dialog = new Microsoft.Win32.OpenFolderDialog
            {
                Title = $"Select game folder for \"{entry.GameName}\" (used for LAN sharing)",
                Multiselect = false
            };
            if (dialog.ShowDialog() != true) return;

            entry.GameFolderPath = dialog.FolderName;
            _library.Update(entry);
            _lanServer.InvalidateManifestCache();

            MessageBox.Show($"Game folder set.\n{entry.GameName} is now shared on LAN.",
                "LAN Share", MessageBoxButton.OK, MessageBoxImage.Information);
        }

        private void LanDownload_Click(object sender, RoutedEventArgs e)
        {
            var dlg = new LanDownloadWindow(_config, _library);
            dlg.Owner = this;
            dlg.ShowDialog();

            // Refresh library in case user added a game during download
            LoadGames();
        }
    }
}
