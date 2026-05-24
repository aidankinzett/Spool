using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public partial class LanDownloadWindow : Wpf.Ui.Controls.FluentWindow
    {
        private readonly Config _config;
        private readonly GameLibrary _library;
        private readonly LanShareClient _client;

        private List<LanPeer> _peers = new();
        private LanPeer? _selectedPeer;
        private string? _selectedGame;
        private CancellationTokenSource? _downloadCts;
        private bool _downloadComplete;

        public LanDownloadWindow(Config config, GameLibrary library)
        {
            InitializeComponent();
            _config  = config;
            _library = library;
            _client  = new LanShareClient(config.Data.DeviceName, config.Data.DeviceId);

            Loaded += async (_, _) => await ScanAsync();
        }

        private async Task ScanAsync()
        {
            ScanningPanel.Visibility  = Visibility.Visible;
            PeerTree.Visibility       = Visibility.Collapsed;
            NoPeersLabel.Visibility   = Visibility.Collapsed;
            RescanButton.IsEnabled    = false;
            DestCard.Visibility       = Visibility.Collapsed;
            DownloadButton.IsEnabled  = false;

            try
            {
                int discoveryPort = _config.Data.LanSharePort - 1;
                _peers = await _client.DiscoverPeersAsync(discoveryPort);
            }
            catch (Exception ex)
            {
                App.Log($"LAN discovery error: {ex.Message}");
                _peers = new();
            }

            ScanningPanel.Visibility = Visibility.Collapsed;
            RescanButton.IsEnabled   = true;

            PopulatePeerTree();
        }

        private void PopulatePeerTree()
        {
            PeerTree.Items.Clear();
            if (_peers.Count == 0)
            {
                NoPeersLabel.Visibility = Visibility.Visible;
                return;
            }

            NoPeersLabel.Visibility = Visibility.Collapsed;
            PeerTree.Visibility     = Visibility.Visible;

            foreach (var peer in _peers)
            {
                var peerItem = new TreeViewItem
                {
                    Header     = $"{peer.DeviceName}  ({peer.IPAddress})",
                    IsExpanded = true,
                    Tag        = peer
                };

                foreach (var game in peer.Games)
                {
                    var gameItem = new TreeViewItem { Header = game, Tag = (peer, game) };
                    peerItem.Items.Add(gameItem);
                }

                PeerTree.Items.Add(peerItem);
            }
        }

        private void PeerTree_SelectedItemChanged(object sender, RoutedPropertyChangedEventArgs<object> e)
        {
            if (PeerTree.SelectedItem is not TreeViewItem { Tag: (LanPeer peer, string game) } item)
            {
                _selectedPeer = null;
                _selectedGame = null;
                DestCard.Visibility      = Visibility.Collapsed;
                DownloadButton.IsEnabled = false;
                return;
            }

            _selectedPeer = peer;
            _selectedGame = game;
            ShowDestinationCard(game);
        }

        private void ShowDestinationCard(string gameName)
        {
            DestCard.Visibility = Visibility.Visible;
            AlreadyInstalledBanner.Visibility = Visibility.Collapsed;

            // Check if game already exists locally with a folder path
            var existing = _library.FindByName(gameName);
            if (existing != null && !string.IsNullOrEmpty(existing.GameFolderPath) && Directory.Exists(existing.GameFolderPath))
            {
                AlreadyInstalledLabel.Text = $"\"{gameName}\" is already installed at:\n{existing.GameFolderPath}\n\nRemove it from your library first to download again.";
                AlreadyInstalledBanner.Visibility = Visibility.Visible;
                DestFieldsGrid.IsEnabled   = false;
                DownloadButton.IsEnabled   = false;
                return;
            }

            DestFieldsGrid.IsEnabled = true;

            // Default destination folder
            string defaultDir = string.IsNullOrEmpty(_config.Data.LanInstallDir)
                ? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), LauncherGenerator.MakeSafeFilename(gameName))
                : Path.Combine(_config.Data.LanInstallDir, LauncherGenerator.MakeSafeFilename(gameName));

            DestFolderBox.Text       = defaultDir;
            DownloadButton.IsEnabled = true;
        }

        private void BrowseDest_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFolderDialog
            {
                Title = "Select destination folder for game files",
                Multiselect = false
            };
            if (dialog.ShowDialog() == true)
                DestFolderBox.Text = dialog.FolderName;
        }

        private async void Rescan_Click(object sender, RoutedEventArgs e) => await ScanAsync();

        private async void Download_Click(object sender, RoutedEventArgs e)
        {
            if (_selectedPeer == null || _selectedGame == null) return;
            string dest = DestFolderBox.Text.Trim();
            if (string.IsNullOrEmpty(dest)) return;

            SetDownloading(true);

            _downloadCts = new CancellationTokenSource();
            var ct = _downloadCts.Token;

            var progress = new Progress<LanDownloadProgress>(p =>
            {
                ProgressFileLabel.Text  = Path.GetFileName(p.CurrentFile);
                ProgressCountLabel.Text = $"{p.FilesCompleted} / {p.TotalFiles} files";

                double pct = p.TotalBytes > 0 ? (double)p.BytesTransferred / p.TotalBytes * 100 : 0;
                ProgressBar.Value = pct;

                ProgressBytesLabel.Text  = $"{FormatBytes(p.BytesTransferred)} / {FormatBytes(p.TotalBytes)}";
                ProgressSpeedLabel.Text  = $"{FormatBytes((long)p.SpeedBytesPerSec)}/s";
            });

            try
            {
                await _client.DownloadGameAsync(_selectedPeer, _selectedGame, dest, progress, ct);
                _downloadComplete = true;

                ProgressFileLabel.Text  = "Download complete";
                ProgressCountLabel.Text = "";
                ProgressBar.Value       = 100;
                ProgressSpeedLabel.Text = "";

                OfferAddToLibrary(_selectedGame, dest);
            }
            catch (OperationCanceledException)
            {
                ProgressFileLabel.Text = "Download cancelled";
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Download failed: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                App.Log($"LAN download error: {ex}");
            }
            finally
            {
                SetDownloading(false);
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
        }

        private void SetDownloading(bool active)
        {
            DiscoveryCard.IsEnabled  = !active;
            DestCard.IsEnabled       = !active;
            DownloadButton.IsEnabled = !active;
            CloseButton.Content      = active ? "Cancel" : (_downloadComplete ? "Done" : "Close");
            ProgressCard.Visibility  = Visibility.Visible;
        }

        private void Close_Click(object sender, RoutedEventArgs e)
        {
            _downloadCts?.Cancel();
            Close();
        }

        private static string FormatBytes(long bytes)
        {
            if (bytes >= 1_073_741_824) return $"{bytes / 1_073_741_824.0:F1} GB";
            if (bytes >= 1_048_576)     return $"{bytes / 1_048_576.0:F1} MB";
            if (bytes >= 1024)          return $"{bytes / 1024.0:F1} KB";
            return $"{bytes} B";
        }
    }
}
