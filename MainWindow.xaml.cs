using System;
using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
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
            if (value is not string path || string.IsNullOrEmpty(path))
                return null;
            try
            {
                var bi = new BitmapImage();
                bi.BeginInit();
                bi.UriSource = new Uri(path);
                if (path.StartsWith("http://", StringComparison.OrdinalIgnoreCase) || path.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
                {
                    bi.CacheOption = BitmapCacheOption.Default;
                }
                else
                {
                    if (!File.Exists(path))
                        return null;
                    bi.CacheOption = BitmapCacheOption.OnLoad;
                }
                bi.EndInit();
                if (!path.StartsWith("http://", StringComparison.OrdinalIgnoreCase) && !path.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
                {
                    bi.Freeze();
                }
                return bi;
            }
            catch (Exception ex)
            {
                App.Log($"StringToImageConverter exception for '{path}': {ex.Message}");
                return null;
            }
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class BooleanToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is bool b)
                return b ? Visibility.Visible : Visibility.Collapsed;
            return Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class InverseBooleanToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is bool b)
                return b ? Visibility.Collapsed : Visibility.Visible;
            return Visibility.Visible;
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
        private CancellationTokenSource? _scanCts;
        private CancellationTokenSource? _uiCleanupCts;
        private int _scanInProgress = 0;

        public static readonly DependencyProperty IsLanScanningProperty =
            DependencyProperty.Register(nameof(IsLanScanning), typeof(bool), typeof(MainWindow), new PropertyMetadata(false));

        public bool IsLanScanning
        {
            get => (bool)GetValue(IsLanScanningProperty);
            set => SetValue(IsLanScanningProperty, value);
        }

        public static readonly DependencyProperty LanPeersCountProperty =
            DependencyProperty.Register(nameof(LanPeersCount), typeof(int), typeof(MainWindow), new PropertyMetadata(0));

        public int LanPeersCount
        {
            get => (int)GetValue(LanPeersCountProperty);
            set => SetValue(LanPeersCountProperty, value);
        }

        public MainWindow(Config config, GameLibrary library)
        {
            InitializeComponent();
            _config = config;
            _library = library;
            Title = $"Ludusavi Wrap v{Version}";

            _lanServer = new LanShareServer(config.Data.DeviceName, config.Data.DeviceId);
            _lanServer.UploadsChanged += LanServer_UploadsChanged;
            _lanServer.PeerActivityDetected += LanServer_PeerActivityDetected;
            _lanClient = new LanShareClient(config.Data.DeviceName, config.Data.DeviceId);

            GamesGrid.ItemsSource = _games;
            LoadGames();
            Loaded  += MainWindow_Loaded;
            Closing += MainWindow_Closing;
        }

        private void LoadGames()
        {
            var lanCards = _games.Where(g => g.IsLanCard).ToList();
            _games.Clear();
            foreach (var entry in _library.Entries)
                _games.Add(entry);
            foreach (var lanCard in lanCards)
                _games.Add(lanCard);
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
            _scanCts?.Cancel();
            _scanCts?.Dispose();
            _uiCleanupCts?.Cancel();
            _uiCleanupCts?.Dispose();
            _downloadCts?.Dispose();
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

            _scanCts = new CancellationTokenSource();
            _ = RunLanScanLoopAsync();

            if (System.Version.TryParse(Version, out var parsedVersion))
                AutoUpdaterDotNET.AutoUpdater.InstalledVersion = parsedVersion;

            AutoUpdaterDotNET.AutoUpdater.ShowSkipButton = false;
            AutoUpdaterDotNET.AutoUpdater.ShowRemindLaterButton = false;
            AutoUpdaterDotNET.AutoUpdater.RunUpdateAsAdmin = false;
            AutoUpdaterDotNET.AutoUpdater.SetOwner(this);

            if (!_updateCheckSubscribed)
            {
                _updateCheckSubscribed = true;
                AutoUpdaterDotNET.AutoUpdater.CheckForUpdateEvent += (args) =>
                {
                    if (args.Error != null || !args.IsUpdateAvailable) return;

                    var result = MessageBox.Show(
                        $"Version {args.CurrentVersion} is available (you have {args.InstalledVersion}).\n\nInstall now? The app will restart automatically.",
                        "Update Available",
                        MessageBoxButton.YesNo,
                        MessageBoxImage.Information);

                    if (result != MessageBoxResult.Yes) return;

                    try
                    {
                        args.InstallerArgs = "/VERYSILENT /SUPPRESSMSGBOXES";
                        if (AutoUpdaterDotNET.AutoUpdater.DownloadUpdate(args))
                            Application.Current.Shutdown();
                    }
                    catch (Exception ex)
                    {
                        MessageBox.Show($"Update failed: {ex.Message}", "Update Error",
                            MessageBoxButton.OK, MessageBoxImage.Error);
                    }
                };
            }

            AutoUpdaterDotNET.AutoUpdater.Start(
                "https://raw.githubusercontent.com/aidankinzett/ludusavi-wrap/master/update.xml");
        }

        private void Settings_Click(object sender, RoutedEventArgs e)
        {
            var setup = new SetupWindow(_config);
            setup.Owner = this;
            if (setup.ShowDialog() == true)
            {
                _lanServer.Stop();
                StartLanServerIfEnabled();
                _ = ScanLanPeersAsync();
            }
        }

        private async Task RunLanScanLoopAsync()
        {
            while (_scanCts != null && !_scanCts.IsCancellationRequested)
            {
                await ScanLanPeersAsync();
                try
                {
                    await Task.Delay(TimeSpan.FromSeconds(30), _scanCts.Token);
                }
                catch (OperationCanceledException)
                {
                    break;
                }
            }
        }

        private async Task ScanLanPeersAsync()
        {
            if (Interlocked.CompareExchange(ref _scanInProgress, 1, 0) != 0) return;
            Dispatcher.Invoke(() => IsLanScanning = true);
            try
            {
                int discoveryPort = _config.Data.LanSharePort - 1;
                var peers = await _lanClient.DiscoverPeersAsync(discoveryPort, _scanCts?.Token ?? default);
                Dispatcher.Invoke(() => LanPeersCount = peers.Count);
                MergeLanGames(peers);
            }
            catch (Exception ex)
            {
                App.Log($"Error during background LAN scan: {ex.Message}");
            }
            finally
            {
                Interlocked.Exchange(ref _scanInProgress, 0);
                Dispatcher.Invoke(() => IsLanScanning = false);
            }
        }

        private void LanServer_PeerActivityDetected(object? sender, EventArgs e)
        {
            _ = ScanLanPeersAsync();
        }

        private void MergeLanGames(System.Collections.Generic.List<LanPeer> peers)
        {
            var lanGames = new System.Collections.Generic.Dictionary<string, System.Collections.Generic.List<LanPeer>>(StringComparer.OrdinalIgnoreCase);
            foreach (var peer in peers)
            {
                foreach (var gameName in peer.Games)
                {
                    if (!lanGames.TryGetValue(gameName, out var peerList))
                    {
                        peerList = new System.Collections.Generic.List<LanPeer>();
                        lanGames[gameName] = peerList;
                    }
                    peerList.Add(peer);
                }
            }

            var localInstalledGames = _library.Entries
                .Where(e => !string.IsNullOrEmpty(e.GameFolderPath) && Directory.Exists(e.GameFolderPath))
                .Select(e => e.GameName)
                .ToHashSet(StringComparer.OrdinalIgnoreCase);

            Dispatcher.BeginInvoke(new Action(() =>
            {
                var toRemove = _games
                    .Where(g => g.IsLanCard && (!lanGames.ContainsKey(g.GameName) || localInstalledGames.Contains(g.GameName)))
                    .ToList();
                foreach (var g in toRemove)
                {
                    _games.Remove(g);
                }

                foreach (var kvp in lanGames)
                {
                    string gameName = kvp.Key;
                    var gamePeers = kvp.Value;

                    if (localInstalledGames.Contains(gameName))
                        continue;

                    var firstPeer = gamePeers.First();
                    string coverUrl = $"http://{firstPeer.IPAddress}:{firstPeer.Port}/games/{Uri.EscapeDataString(gameName)}/cover";

                    var existingCard = _games.FirstOrDefault(g => g.IsLanCard && string.Equals(g.GameName, gameName, StringComparison.OrdinalIgnoreCase));
                    if (existingCard != null)
                    {
                        existingCard.LanPeers = gamePeers;
                        existingCard.CoverImagePath = coverUrl;
                    }
                    else
                    {
                        _games.Add(new GameEntry
                        {
                            GameName = gameName,
                            SafeName = LauncherGenerator.MakeSafeFilename(gameName),
                            CoverImagePath = coverUrl,
                            IsLanCard = true,
                            LanPeers = gamePeers
                        });
                    }
                }

                UpdateEmptyState();
            }));
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
                _lanServer.BroadcastAnnounce();
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

                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                {
                    _successWindow.UpdateArtwork("Fetching cover image...", "#99FFFFFF");
                    _ = Task.Run(() => FetchCoverArtForACAsync(entry.GameName, entry.SafeName));
                }

                _successWindow.ShowDialog();
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to generate launcher: {ex.Message}",
                    "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private async Task FetchCoverArtForACAsync(string gameName, string safeName)
        {
            string coversDir = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "ludusavi-wrap", "covers");

            try
            {
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(gameName);
                if (results.Count == 0)
                {
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Artwork: Game not found on SteamGridDB", "#FFC107"));
                    return;
                }

                int gameId = results[0].Id;
                string? imgPath = await sgdb.DownloadGridImageAsync(gameId, safeName, coversDir);
                if (imgPath == null)
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Artwork: No horizontal grid images found on SteamGridDB", "#FFC107"));
                else
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"Cover art: {imgPath}", "#4CAF50"));
            }
            catch (Exception ex)
            {
                App.Log($"[MainWindow] Failed to fetch cover art for Armoury Crate '{gameName}': {ex.Message}");
                Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ Artwork error: {ex.Message}", "#FFC107"));
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
            _lanServer.BroadcastAnnounce();
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
            _lanServer.BroadcastAnnounce();

            MessageBox.Show($"Game folder set.\n{entry.GameName} is now shared on LAN.",
                "LAN Share", MessageBoxButton.OK, MessageBoxImage.Information);
        }

        private async void DownloadCard_Click(object sender, RoutedEventArgs e)
        {
            if (_isDownloading)
            {
                MessageBox.Show("A download is already in progress. Please wait for it to finish or cancel it first.",
                    "Download in Progress", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            if ((sender as Button)?.DataContext is not GameEntry entry || !entry.IsLanCard || entry.LanPeers == null || entry.LanPeers.Count == 0)
                return;

            var dlg = new DownloadLocationWindow(_config, _library, entry.GameName);
            dlg.Owner = this;
            if (dlg.ShowDialog() == true && !string.IsNullOrEmpty(dlg.DestFolder))
            {
                await StartDownloadWithPeersAsync(entry.LanPeers, entry.GameName, dlg.DestFolder);
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

        private async Task StartDownloadWithPeersAsync(System.Collections.Generic.List<LanPeer> peers, string gameName, string destFolder)
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

            _downloadCts?.Dispose();
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

            Exception? lastEx = null;
            bool success = false;
            LanPeer? successPeer = null;
            try
            {
                foreach (var peer in peers)
                {
                    if (ct.IsCancellationRequested) break;
                    try
                    {
                        App.Log($"Attempting LAN download from peer: {peer.DeviceName} ({peer.IPAddress}:{peer.Port})");
                        await _lanClient.DownloadGameAsync(peer, gameName, destFolder, progress, ct);
                        success = true;
                        successPeer = peer;
                        break;
                    }
                    catch (OperationCanceledException)
                    {
                        throw;
                    }
                    catch (Exception ex)
                    {
                        App.Log($"Download from peer {peer.DeviceName} failed: {ex.Message}");
                        lastEx = ex;
                    }
                }

                if (success)
                {
                    DownloadTitleText.Text = "Download complete";
                    DownloadCountText.Text = "";
                    DownloadProgressBar.Value = 100;
                    DownloadSpeedText.Text = "";
                    DownloadBytesText.Text = "";

                    OfferAddToLibrary(gameName, destFolder, successPeer);
                }
                else if (ct.IsCancellationRequested)
                {
                    DownloadTitleText.Text = "Download cancelled";
                    DownloadCountText.Text = "";
                    DownloadSpeedText.Text = "";
                    DownloadBytesText.Text = "";
                }
                else
                {
                    string errorMsg = lastEx?.Message ?? "No peers available";
                    MessageBox.Show($"Download failed: {errorMsg}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                    DownloadTitleText.Text = "Download failed";
                    DownloadCountText.Text = "";
                    DownloadSpeedText.Text = "";
                    DownloadBytesText.Text = "";
                }
            }
            catch (OperationCanceledException)
            {
                bool hostCancelled = !ct.IsCancellationRequested;
                DownloadTitleText.Text = hostCancelled ? "Cancelled by host" : "Download cancelled";
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
                _uiCleanupCts?.Cancel();
                _uiCleanupCts?.Dispose();
                _uiCleanupCts = new CancellationTokenSource();
                var cleanupToken = _uiCleanupCts.Token;
                _ = Task.Delay(TimeSpan.FromSeconds(5), cleanupToken).ContinueWith(t =>
                {
                    if (cleanupToken.IsCancellationRequested) return;
                    Dispatcher.Invoke(() =>
                    {
                        if (!_isDownloading)
                        {
                            DownloadSeparator.Visibility = Visibility.Collapsed;
                            DownloadBarGrid.Visibility = Visibility.Collapsed;
                        }
                    });
                }, TaskScheduler.Default);
            }
        }

        private void OfferAddToLibrary(string gameName, string destFolder, LanPeer? sourcePeer = null)
        {
            string? exePath = Directory.GetFiles(destFolder, "*.exe", SearchOption.AllDirectories)
                .OrderBy(f => Path.GetDirectoryName(f)?.Length)
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

            var entry = _library.FindByName(gameName);
            if (entry != null && string.IsNullOrEmpty(entry.CoverImagePath))
                _ = Task.Run(() => FetchCoverForLanEntryAsync(entry, sourcePeer));
        }

        private async Task FetchCoverForLanEntryAsync(GameEntry entry, LanPeer? peer)
        {
            string coversDir = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "ludusavi-wrap", "covers");
            Directory.CreateDirectory(coversDir);

            // Try pulling the cover directly from the peer that served the game files.
            if (peer != null)
            {
                try
                {
                    using var http = new System.Net.Http.HttpClient { Timeout = TimeSpan.FromSeconds(15) };
                    string url = $"http://{peer.IPAddress}:{peer.Port}/games/{Uri.EscapeDataString(entry.GameName)}/cover";
                    using var response = await http.GetAsync(url);
                    if (response.IsSuccessStatusCode)
                    {
                        string ext = response.Content.Headers.ContentType?.MediaType switch
                        {
                            "image/png"  => ".png",
                            "image/webp" => ".webp",
                            "image/gif"  => ".gif",
                            _            => ".jpg"
                        };
                        string imagePath = Path.Combine(coversDir, entry.SafeName + "_p" + ext);
                        byte[] bytes = await response.Content.ReadAsByteArrayAsync();
                        await File.WriteAllBytesAsync(imagePath, bytes);
                        Dispatcher.Invoke(() => { entry.CoverImagePath = imagePath; _library.Update(entry); });
                        App.Log($"Downloaded cover from LAN peer for '{entry.GameName}'");
                        return;
                    }
                }
                catch (Exception ex)
                {
                    App.Log($"LAN cover fetch failed for '{entry.GameName}': {ex.Message}");
                }
            }

            // Fall back to SteamGridDB if configured.
            if (!_config.Data.SteamGridDbEnabled || string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey)) return;
            try
            {
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(entry.GameName);
                if (results.Count == 0) return;

                string destBase = Path.Combine(coversDir, entry.SafeName);
                string? imagePath = await sgdb.DownloadPortraitAsync(results[0].Id, destBase + "_p");
                imagePath ??= await sgdb.DownloadGridImageAsync(results[0].Id, entry.SafeName, coversDir);
                if (imagePath != null)
                    Dispatcher.Invoke(() => { entry.CoverImagePath = imagePath; _library.Update(entry); });
            }
            catch (Exception ex)
            {
                App.Log($"SteamGridDB cover fetch failed for '{entry.GameName}': {ex.Message}");
            }
        }

        private void CancelDownload_Click(object sender, RoutedEventArgs e)
        {
            _downloadCts?.Cancel();
        }
    }
}
