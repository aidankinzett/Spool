using System;
using System.IO;
using System.Windows;
using Microsoft.Win32;
using Wpf.Ui.Appearance;

namespace LudusaviWrap
{
    public partial class DownloadLocationWindow : Wpf.Ui.Controls.FluentWindow
    {
        private readonly Config _config;
        private readonly GameLibrary _library;
        private readonly string _gameName;

        public string? DestFolder { get; private set; }

        public DownloadLocationWindow(Config config, GameLibrary library, string gameName)
        {
            SystemThemeWatcher.Watch(this);
            InitializeComponent();
            ThemeManager.ApplyTheme(config.Data.Theme);

            _config = config;
            _library = library;
            _gameName = gameName;

            GameNameLabel.Text = gameName;

            CheckInstallationStatus();
        }

        private void CheckInstallationStatus()
        {
            var existing = _library.FindByName(_gameName);
            if (existing != null && !string.IsNullOrEmpty(existing.GameFolderPath) && Directory.Exists(existing.GameFolderPath))
            {
                AlreadyInstalledLabel.Text = $"\"{_gameName}\" is already installed at:\n{existing.GameFolderPath}\n\nRemove it from your library first to download again.";
                AlreadyInstalledBanner.Visibility = Visibility.Visible;
                DestCard.Visibility = Visibility.Collapsed;
                DownloadButton.IsEnabled = false;
            }
            else
            {
                AlreadyInstalledBanner.Visibility = Visibility.Collapsed;
                DestCard.Visibility = Visibility.Visible;
                DownloadButton.IsEnabled = true;

                // Default destination folder
                string defaultDir = string.IsNullOrEmpty(_config.Data.LanInstallDir)
                    ? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), LauncherGenerator.MakeSafeFilename(_gameName))
                    : Path.Combine(_config.Data.LanInstallDir, LauncherGenerator.MakeSafeFilename(_gameName));

                DestFolderBox.Text = defaultDir;
            }
        }

        private void BrowseDest_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFolderDialog
            {
                Title = $"Select folder to install \"{_gameName}\"",
                Multiselect = false
            };
            if (dialog.ShowDialog() == true)
            {
                DestFolderBox.Text = dialog.FolderName;
            }
        }

        private void Download_Click(object sender, RoutedEventArgs e)
        {
            string dest = DestFolderBox.Text.Trim();
            if (string.IsNullOrEmpty(dest))
            {
                MessageBox.Show("Please select a valid installation directory.", "Invalid Path", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }

            DestFolder = dest;
            DialogResult = true;
            Close();
        }

        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            DialogResult = false;
            Close();
        }
    }
}
