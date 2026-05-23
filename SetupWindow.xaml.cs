using System;
using System.Diagnostics;
using System.IO;
using System.Windows;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public partial class SetupWindow : Window
    {
        private readonly Config _config;
        private readonly bool _isFirstRun;

        public SetupWindow(Config config, bool isFirstRun = false)
        {
            InitializeComponent();
            _config = config;
            _isFirstRun = isFirstRun;

            // Load values from configuration
            LudusaviPathTextBox.Text = _config.Data.LudusaviPath;
            SgdbSwitch.IsChecked = _config.Data.SteamGridDbEnabled;
            ApiKeyTextBox.Text = _config.Data.SteamGridDbApiKey;

            // Update UI state based on switch
            UpdateSgdbUiState();
        }

        private void SgdbSwitch_Changed(object sender, RoutedEventArgs e)
        {
            UpdateSgdbUiState();
        }

        private void UpdateSgdbUiState()
        {
            if (ApiKeyGrid != null && ApiKeyTextBox != null)
            {
                bool enabled = SgdbSwitch.IsChecked ?? false;
                ApiKeyTextBox.IsEnabled = enabled;
            }
        }

        private void BrowseLudusavi_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFileDialog
            {
                Title = "Select ludusavi.exe",
                Filter = "ludusavi.exe|ludusavi.exe|Executables (*.exe)|*.exe",
                RestoreDirectory = true
            };

            if (dialog.ShowDialog() == true)
            {
                LudusaviPathTextBox.Text = dialog.FileName;
            }
        }

        private void GetKey_Click(object sender, RoutedEventArgs e)
        {
            try
            {
                Process.Start(new ProcessStartInfo
                {
                    FileName = "https://www.steamgriddb.com/profile/preferences/api",
                    UseShellExecute = true
                });
            }
            catch (Exception ex)
            {
                ShowError($"Could not open website: {ex.Message}");
            }
        }

        private void Save_Click(object sender, RoutedEventArgs e)
        {
            string path = LudusaviPathTextBox.Text.Trim();
            bool sgdbEnabled = SgdbSwitch.IsChecked ?? false;
            string apiKey = ApiKeyTextBox.Text.Trim();

            if (string.IsNullOrEmpty(path) || !File.Exists(path))
            {
                ShowError("Please select a valid ludusavi.exe file.");
                return;
            }

            if (sgdbEnabled && string.IsNullOrEmpty(apiKey))
            {
                ShowError("API key is required when SteamGridDB is enabled.");
                return;
            }

            // Save back to configuration
            _config.Data.LudusaviPath = path;
            _config.Data.SteamGridDbEnabled = sgdbEnabled;
            _config.Data.SteamGridDbApiKey = apiKey;
            _config.Save();

            DialogResult = true;
            Close();
        }

        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            DialogResult = false;
            Close();
        }

        private void ShowError(string message)
        {
            ErrorLabel.Text = message;
            ErrorLabel.Visibility = Visibility.Visible;
        }
    }
}
