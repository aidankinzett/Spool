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
        private readonly LanShareClient _lanClient;

        public MainWindow(Config config, GameLibrary library)
        {
            InitializeComponent();
            _config = config;
            _library = library;
            Title = $"Ludusavi Wrap v{Version}";

            _lanServer = new LanShareServer(config.Data.DeviceName, config.Data.DeviceId);
            _lanServer.UploadsChanged += LanServer_UploadsChanged;
            _lanClient = new LanShareClient(config.Data.DeviceName, config.Data.DeviceId);

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

        private async void LanDownload_Click(object sender, RoutedEventArgs e)
        {
            if (_isDownloading)
            {
                MessageBox.Show("A download is already in progress. Please wait for it to finish or cancel it first.",
                    "Download in Progress", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            var dlg = new LanDownloadWindow(_config, _library);
            dlg.Owner = this;
            if (dlg.ShowDialog() == true && dlg.StartDownloadRequested)
            {
                var peer = dlg.SelectedPeer;
                var game = dlg.SelectedGame;
                var dest = dlg.DestFolder;
                if (peer != null && !string.IsNullOrEmpty(game) && !string.IsNullOrEmpty(dest))
                {
                    await StartDownloadAsync(peer, game, dest);
                }
            }
            else
            {
                LoadGames();
            }
        }

        private System.Windows.Threading.DispatcherTimer? _uploadTimer;
        private long _lastBytesSent = 0;
        private DateTime _lastUploadTick = DateTime.MinValue;

        private void LanServer_UploadsChanged(object? sender, EventArgs e)
        {
            if (_uploadTimer == null)
            {
                Dispatcher.BeginInvoke(new Action(UpdateUploadProgress));
            }
        }

        private void UpdateUploadProgress()
        {
            var uploads = _lanServer.GetActiveUploads();
            if (uploads.Count == 0)
            {
                if (_uploadTimer != null)
                {
                    _uploadTimer.Stop();
                    _uploadTimer = null;
                }
                UploadSeparator.Visibility = Visibility.Collapsed;
                UploadBarGrid.Visibility = Visibility.Collapsed;
                _lastBytesSent = 0;
                _lastUploadTick = DateTime.MinValue;
                return;
            }

            // Make sure the timer is running
            if (_uploadTimer == null)
            {
                _uploadTimer = new System.Windows.Threading.DispatcherTimer
                {
                    Interval = TimeSpan.FromMilliseconds(500)
                };
                _uploadTimer.Tick += (s, e) => UpdateUploadProgress();
                _uploadTimer.Start();
                _lastUploadTick = DateTime.UtcNow;
                _lastBytesSent = 0;
            }

            // Calculate total stats
            long totalBytes = 0;
            long bytesSent = 0;
            string gameName = "";

            foreach (var u in uploads)
            {
                totalBytes += u.TotalBytes;
                bytesSent += u.BytesSent;
                if (string.IsNullOrEmpty(gameName))
                    gameName = u.GameName;
            }

            // If multiple games are uploading, aggregate
            var gameNames = uploads.Select(u => u.GameName).Distinct().ToList();
            if (gameNames.Count > 1)
            {
                UploadTitleText.Text = $"Uploading {gameNames.Count} games to LAN peer...";
            }
            else
            {
                UploadTitleText.Text = $"Uploading {gameName} to LAN peer...";
            }

            // Progress bar
            double progress = totalBytes > 0 ? (bytesSent * 100.0 / totalBytes) : 0;
            UploadProgressBar.Value = progress;

            // Speed calculation
            double speed = 0;
            var now = DateTime.UtcNow;
            if (_lastUploadTick != DateTime.MinValue && bytesSent >= _lastBytesSent)
            {
                double elapsed = (now - _lastUploadTick).TotalSeconds;
                if (elapsed > 0)
                {
                    speed = Math.Max(0, (bytesSent - _lastBytesSent) / elapsed);
                }
            }

            _lastBytesSent = bytesSent;
            _lastUploadTick = now;

            // Status text: Speed + Progress bytes
            string speedStr = FormatSpeed(speed);
            string progressStr = $"{FormatBytes(bytesSent)} of {FormatBytes(totalBytes)}";
            UploadStatusText.Text = $"{speedStr} - {progressStr}";

            UploadSeparator.Visibility = Visibility.Visible;
            UploadBarGrid.Visibility = Visibility.Visible;
        }

        private static string FormatBytes(long bytes)
        {
            string[] suffixes = { "B", "KB", "MB", "GB", "TB" };
            double val = bytes;
            int i = 0;
            while (val >= 1024 && i < suffixes.Length - 1)
            {
                val /= 1024;
                i++;
            }
            return $"{val:0.0} {suffixes[i]}";
        }

        private static string FormatSpeed(double bytesPerSec)
        {
            string[] suffixes = { "B/s", "KB/s", "MB/s", "GB/s" };
            double val = bytesPerSec;
            int i = 0;
            while (val >= 1024 && i < suffixes.Length - 1)
            {
                val /= 1024;
                i++;
            }
            return $"{val:0.0} {suffixes[i]}";
        }

        private void CancelUpload_Click(object sender, RoutedEventArgs e)
        {
            _lanServer.CancelAllUploads();
        }

        private CancellationTokenSource? _downloadCts;
        private bool _isDownloading = false;

        private async Task StartDownloadAsync(LanPeer peer, string gameName, string destFolder)
        {
            _isDownloading = true;
            DownloadSeparator.Visibility = Visibility.Visible;
            DownloadBarGrid.Visibility = Visibility.Visible;
            CancelDownloadButton.Visibility = Visibility.Visible;

            DownloadTitleText.Text = "Preparing...";
            DownloadCountText.Text = "";
            DownloadSpeedText.Text = "";
            DownloadBytesText.Text = "";
            DownloadProgressBar.Value = 0;

            _downloadCts = new CancellationTokenSource();
            var ct = _downloadCts.Token;

            var progress = new Progress<LanDownloadProgress>(p =>
            {
                if (!string.IsNullOrEmpty(p.Status) && p.Status != "Downloading")
                {
                    DownloadTitleText.Text = p.Status;
                    if (p.Status.StartsWith("Verifying"))
                    {
                        DownloadBytesText.Text = Path.GetFileName(p.CurrentFile);
                        double pct = p.TotalBytes > 0 ? (double)p.BytesTransferred / p.TotalBytes * 100 : 0;
                        DownloadProgressBar.Value = pct;
                    }
                    else
                    {
                        DownloadBytesText.Text = "";
                        DownloadProgressBar.Value = 0;
                    }
                    DownloadCountText.Text = "";
                    DownloadSpeedText.Text = "";
                }
                else
                {
                    DownloadTitleText.Text = $"Downloading {Path.GetFileName(p.CurrentFile)}";
                    DownloadCountText.Text = $"({p.FilesCompleted} / {p.TotalFiles} files)";

                    double pct = p.TotalBytes > 0 ? (double)p.BytesTransferred / p.TotalBytes * 100 : 0;
                    DownloadProgressBar.Value = pct;

                    DownloadBytesText.Text = $"{FormatBytes(p.BytesTransferred)} of {FormatBytes(p.TotalBytes)}";
                    DownloadSpeedText.Text = $"{FormatSpeed(p.SpeedBytesPerSec)}";
                }
            });

            try
            {
                await _lanClient.DownloadGameAsync(peer, gameName, destFolder, progress, ct);

                DownloadTitleText.Text = "Download complete";
                DownloadCountText.Text = "";
                DownloadProgressBar.Value = 100;
                DownloadSpeedText.Text = "";
                DownloadBytesText.Text = "";

                OfferAddToLibrary(gameName, destFolder);
            }
            catch (OperationCanceledException)
            {
                DownloadTitleText.Text = "Download cancelled";
                DownloadCountText.Text = "";
                DownloadSpeedText.Text = "";
                DownloadBytesText.Text = "";
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Download failed: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                DownloadTitleText.Text = "Download failed";
                DownloadCountText.Text = "";
                DownloadSpeedText.Text = "";
                DownloadBytesText.Text = "";
            }
            finally
            {
                _isDownloading = false;
                CancelDownloadButton.Visibility = Visibility.Collapsed;
            }
        }

        private void OfferAddToLibrary(string gameName, string destFolder)
        {
            // Try to find an exe in the destination folder
            string? exePath = Directory.GetFiles(destFolder, "*.exe", SearchOption.AllDirectories)
                .OrderBy(f => Path.GetDirectoryName(f)?.Length) // prefer shallower
                .FirstOrDefault();

            var ans = MessageBox.Show(
                $"Download of \"{gameName}\" is complete.\n\nAdd it to your library?\n" +
                (exePath != null ? $"\nDetected executable:\n{exePath}" : "\n(No .exe found — you can set it manually)"),
                "Add to Library?", MessageBoxButton.YesNo, MessageBoxImage.Question);

            if (ans != MessageBoxResult.Yes) return;

            var existing = _library.FindByName(gameName);
            if (existing != null)
            {
                existing.GameFolderPath = destFolder;
                if (exePath != null) existing.ExePath = exePath;
                _library.Update(existing);
            }
            else
            {
                _library.Add(new GameEntry
                {
                    GameName       = gameName,
                    SafeName       = LauncherGenerator.MakeSafeFilename(gameName),
                    ExePath        = exePath ?? "",
                    GameFolderPath = destFolder,
                    AddedAt        = DateTime.UtcNow
                });
            }
            LoadGames();
        }

        private void CancelDownload_Click(object sender, RoutedEventArgs e)
        {
            _downloadCts?.Cancel();
        }
    }
}
