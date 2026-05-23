using System;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;

namespace LudusaviWrap
{
    public partial class SuccessWindow : Window
    {
        private readonly string _gameName;
        private readonly string _exePath;

        public SuccessWindow(Window owner, string gameName, string exePath)
        {
            InitializeComponent();
            Owner = owner;
            _gameName = gameName;
            _exePath = exePath;

            GameNameTextBox.Text = _gameName;
            LauncherPathTextBox.Text = _exePath;
        }

        private async void CopyText(string text, Button btn)
        {
            try
            {
                Clipboard.SetText(text);
                btn.Content = "✓";
                await System.Threading.Tasks.Task.Delay(1500);
                btn.Content = "Copy";
            }
            catch
            {
                // Clipboard operation can fail in RDP/VM
            }
        }

        private void CopyName_Click(object sender, RoutedEventArgs e)
        {
            CopyText(_gameName, CopyNameButton);
        }

        private void CopyPath_Click(object sender, RoutedEventArgs e)
        {
            CopyText(_exePath, CopyPathButton);
        }

        private void OpenFolder_Click(object sender, RoutedEventArgs e)
        {
            try
            {
                if (File.Exists(_exePath))
                {
                    // Open explorer and highlight the file
                    Process.Start("explorer.exe", $"/select,\"{_exePath}\"");
                }
                else
                {
                    string? dir = Path.GetDirectoryName(_exePath);
                    if (dir != null && Directory.Exists(dir))
                    {
                        Process.Start("explorer.exe", $"\"{dir}\"");
                    }
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Could not open folder: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        public void UpdateArtwork(string text, string colorHex)
        {
            ArtworkStatusLabel.Text = text;
            try
            {
                var converter = new BrushConverter();
                var brush = converter.ConvertFromString(colorHex) as Brush;
                if (brush != null)
                {
                    ArtworkStatusLabel.Foreground = brush;
                }
            }
            catch
            {
                ArtworkStatusLabel.Foreground = SystemColors.WindowTextBrush;
            }
        }

        private void Close_Click(object sender, RoutedEventArgs e)
        {
            Close();
        }
    }
}
