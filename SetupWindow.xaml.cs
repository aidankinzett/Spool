using System;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public partial class SetupWindow : Wpf.Ui.Controls.FluentWindow
    {
        private readonly Config _config;
        private readonly bool _isFirstRun;
        private bool _themeComboInitialized = false;

        public SetupWindow(Config config, bool isFirstRun = false)
        {
            InitializeComponent();
            _config = config;
            _isFirstRun = isFirstRun;

            // Load values from configuration
            LudusaviPathTextBox.Text = _config.Data.LudusaviPath;
            SgdbSwitch.IsChecked = _config.Data.SteamGridDbEnabled;
            ApiKeyTextBox.Text = _config.Data.SteamGridDbApiKey;

            // Populate theme ComboBox
            SelectThemeComboItem(_config.Data.Theme);
            _themeComboInitialized = true;

            // Update UI state based on switch
            UpdateSgdbUiState();
        }

        private void SelectThemeComboItem(string theme)
        {
            foreach (ComboBoxItem item in ThemeComboBox.Items)
            {
                if (item.Tag?.ToString() == theme)
                {
                    ThemeComboBox.SelectedItem = item;
                    return;
                }
            }
            // Fallback to system
            ThemeComboBox.SelectedIndex = 0;
        }

        private void ThemeComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            // Guard against firing during InitializeComponent
            if (!_themeComboInitialized) return;

            if (ThemeComboBox.SelectedItem is ComboBoxItem selected &&
                selected.Tag?.ToString() is string tag)
            {
                // Live preview — apply immediately without saving
                ThemeManager.ApplyTheme(tag);
            }
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

            // Persist theme selection
            string themeValue = "system";
            if (ThemeComboBox.SelectedItem is ComboBoxItem selectedTheme &&
                selectedTheme.Tag?.ToString() is string tag)
            {
                themeValue = tag;
            }

            // Save back to configuration
            _config.Data.LudusaviPath      = path;
            _config.Data.SteamGridDbEnabled = sgdbEnabled;
            _config.Data.SteamGridDbApiKey  = apiKey;
            _config.Data.Theme              = themeValue;
            _config.Save();

            DialogResult = true;
            Close();
        }

        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            // Revert live preview to saved preference
            ThemeManager.ApplyTheme(_config.Data.Theme);
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
