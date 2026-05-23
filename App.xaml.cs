using System;
using System.IO;
using System.Windows;
using System.Windows.Media;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public partial class App : Application
    {
        protected override void OnStartup(StartupEventArgs e)
        {
            base.OnStartup(e);

            string[] args = e.Args;

            // Route 1: CLI wrapper execution (running the game with Ludusavi backing it up)
            if (args.Length > 1 && args[0] == "--run")
            {
                if (args.Length >= 3)
                {
                    string gameName = args[1];
                    string gameExe = args[2];
                    var runWindow = new RunWindow(gameName, gameExe);
                    runWindow.Show();
                }
                else
                {
                    MessageBox.Show("Invalid command-line arguments. Usage: --run <GameName> <GameExe>", 
                                    "Ludusavi Wrap Error", MessageBoxButton.OK, MessageBoxImage.Error);
                    Shutdown();
                }
                return;
            }

            // Route 2: Main GUI (Wrapper Generator)
            var config = new Config();
            
            if (!config.IsLudusaviOk)
            {
                // Force user to setup on first run
                var setupWindow = new SetupWindow(config, isFirstRun: true);
                bool? result = setupWindow.ShowDialog();
                if (result != true)
                {
                    Shutdown();
                    return;
                }
            }

            var mainWindow = new MainWindow(config);
            mainWindow.Show();
        }
    }
}
