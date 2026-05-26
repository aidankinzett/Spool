using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.Http;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;

namespace LudusaviWrap
{
    // ── JSON DTOs ─────────────────────────────────────────────────────────────

    public class LanGameMetadata
    {
        [JsonPropertyName("game_name")]         public string GameName { get; set; } = "";
        [JsonPropertyName("run_as_admin")]      public bool   RunAsAdmin { get; set; }
        [JsonPropertyName("relative_exe_path")] public string RelativeExePath { get; set; } = "";
        [JsonPropertyName("install_size_mb")]   public double InstallSizeMb { get; set; }
    }

    public class LanAnnounce
    {
        [JsonPropertyName("type")]       public string Type       { get; set; } = "announce";
        [JsonPropertyName("deviceName")] public string DeviceName { get; set; } = "";
        [JsonPropertyName("deviceId")]   public string DeviceId   { get; set; } = "";
        [JsonPropertyName("port")]       public int    Port       { get; set; }
        [JsonPropertyName("games")]      public List<string> Games { get; set; } = new();
    }

    public class LanQuery
    {
        [JsonPropertyName("type")] public string Type { get; set; } = "query";
    }

    public class LanFileEntry
    {
        [JsonPropertyName("path")]     public string RelativePath { get; set; } = "";
        [JsonPropertyName("size")]     public long   Size         { get; set; }
        [JsonPropertyName("modified")] public long   LastModified { get; set; }
    }

    public class LanPeer
    {
        public string DeviceName  { get; set; } = "";
        public string DeviceId    { get; set; } = "";
        public string IPAddress   { get; set; } = "";
        public int    Port        { get; set; }
        public List<string> Games { get; set; } = new();
        public override string ToString() => $"{DeviceName} ({IPAddress})";
    }

    public class LanDownloadProgress
    {
        public string Status            { get; set; } = "";
        public string CurrentFile       { get; set; } = "";
        public int    FilesCompleted    { get; set; }
        public int    TotalFiles        { get; set; }
        public long   BytesTransferred  { get; set; }
        public long   TotalBytes        { get; set; }
        public double SpeedBytesPerSec  { get; set; }
    }

    public class ActiveUpload
    {
        public string Guid { get; set; } = System.Guid.NewGuid().ToString();
        public string GameName { get; }
        public string RelativePath { get; set; } = "";
        public long TotalBytes { get; set; }
        public long BytesSent { get; set; }
        public CancellationTokenSource Cts { get; }
        public int ActiveCount { get; set; }
        public DateTime LastActive { get; set; } = DateTime.UtcNow;

        public ActiveUpload(string gameName, string relativePath, long totalBytes, CancellationTokenSource cts)
        {
            GameName = gameName;
            RelativePath = relativePath;
            TotalBytes = totalBytes;
            Cts = cts;
        }
    }

    public class ActiveUploadSnapshot
    {
        public string Guid { get; set; } = "";
        public string GameName { get; set; } = "";
        public string RelativePath { get; set; } = "";
        public long TotalBytes { get; set; }
        public long BytesSent { get; set; }
    }

    [JsonSourceGenerationOptions]
    [JsonSerializable(typeof(LanAnnounce))]
    [JsonSerializable(typeof(LanQuery))]
    [JsonSerializable(typeof(List<LanFileEntry>))]
    [JsonSerializable(typeof(List<string>))]
    [JsonSerializable(typeof(LanGameMetadata))]
    internal partial class LanJsonContext : JsonSerializerContext { }

    // ── Server ────────────────────────────────────────────────────────────────

    public class LanShareServer : IDisposable
    {
        private const int BufferSize = 512 * 1024;

        private readonly string _deviceName;
        private readonly string _deviceId;
        private Func<IEnumerable<GameEntry>>? _gameSource;
        private int _port;
        private int _discoveryPort;

        private TcpListener?  _listener;
        private UdpClient?    _udpServer;
        private CancellationTokenSource? _cts;

        // manifest cache: gameName → list of entries (built once per game, cleared on stop)
        private readonly Dictionary<string, List<LanFileEntry>> _manifestCache = new();
        private readonly object _cacheLock = new();

        private readonly List<ActiveUpload> _activeUploads = new();
        private readonly object _uploadsLock = new();
        public event EventHandler? UploadsChanged;
        public event EventHandler? PeerActivityDetected;

        private readonly HashSet<string> _cancelledGames = new();
        private readonly object _cancelledLock = new();

        private long _lastNotifyTicks = 0;

        private void NotifyUploadsChanged()
        {
            // Throttle to avoid flooding the dispatcher on fast transfers (~10x per second is plenty)
            var now = DateTime.UtcNow.Ticks;
            var last = Interlocked.Read(ref _lastNotifyTicks);
            if (now - last < TimeSpan.TicksPerMillisecond * 100) return;
            Interlocked.Exchange(ref _lastNotifyTicks, now);
            UploadsChanged?.Invoke(this, EventArgs.Empty);
        }

        public List<ActiveUploadSnapshot> GetActiveUploads()
        {
            var now = DateTime.UtcNow;
            lock (_uploadsLock)
            {
                // Clean up stale sessions:
                // - ActiveCount == 0 and inactive for > 3 seconds
                // - OR cancellation has been requested
                _activeUploads.RemoveAll(u => u.Cts.IsCancellationRequested || (u.ActiveCount == 0 && (now - u.LastActive).TotalSeconds > 3));

                return _activeUploads.Select(u => new ActiveUploadSnapshot
                {
                    Guid = u.Guid,
                    GameName = u.GameName,
                    RelativePath = u.RelativePath,
                    TotalBytes = u.TotalBytes,
                    BytesSent = u.BytesSent
                }).ToList();
            }
        }

        public void CancelAllUploads()
        {
            List<ActiveUpload> snapshot;
            lock (_uploadsLock)
            {
                snapshot = _activeUploads.ToList();
            }
            lock (_cancelledLock)
            {
                foreach (var upload in snapshot)
                    _cancelledGames.Add(upload.GameName);
            }
            foreach (var upload in snapshot)
                try { upload.Cts.Cancel(); } catch { }
        }

        public bool IsRunning { get; private set; }

        public Action<string, List<LanFileEntry>>? OnManifestBuilt { get; set; }

        public void SetCachedManifest(string gameName, List<LanFileEntry> manifest)
        {
            lock (_cacheLock)
            {
                _manifestCache[gameName] = manifest;
            }
        }

        public bool TryGetCachedManifest(string gameName, out List<LanFileEntry> manifest)
        {
            lock (_cacheLock)
            {
                return _manifestCache.TryGetValue(gameName, out manifest!);
            }
        }

        public LanShareServer(string deviceName, string deviceId)
        {
            _deviceName = deviceName;
            _deviceId   = deviceId;
        }

        public void Start(Func<IEnumerable<GameEntry>> gameSource, int port)
        {
            if (IsRunning) Stop();

            _gameSource     = gameSource;
            _port           = port;
            _discoveryPort  = port - 1;
            _cts            = new CancellationTokenSource();
            var ct          = _cts.Token;

            _listener = new TcpListener(System.Net.IPAddress.Any, _port);
            _listener.Start();

            _udpServer = new UdpClient(_discoveryPort);
            _udpServer.EnableBroadcast = true;

            IsRunning = true;

            _ = AcceptLoopAsync(ct);
            _ = UdpListenLoopAsync(ct);
            _ = AnnounceLoopAsync(ct);

            App.Log($"LanShareServer started on port {_port}");
        }

        public void Stop()
        {
            _cts?.Cancel();
            _listener?.Stop();
            _udpServer?.Dispose();
            _udpServer = null;
            lock (_cacheLock) _manifestCache.Clear();
            IsRunning = false;
            App.Log("LanShareServer stopped");
        }

        public void InvalidateManifestCache()
        {
            lock (_cacheLock) _manifestCache.Clear();
        }

        public void BroadcastAnnounce()
        {
            if (!IsRunning || _cts == null) return;
            _ = Task.Run(async () =>
            {
                try
                {
                    await BroadcastAnnounceAsync(_cts.Token);
                }
                catch { }
            });
        }

        private async Task AcceptLoopAsync(CancellationToken ct)
        {
            while (!ct.IsCancellationRequested)
            {
                try
                {
                    var client = await _listener!.AcceptTcpClientAsync(ct);
                    client.ReceiveBufferSize = 256 * 1024;
                    client.SendBufferSize    = 256 * 1024;
                    _ = HandleClientAsync(client, ct);
                }
                catch (OperationCanceledException) { break; }
                catch (Exception ex) { App.Log($"LanShareServer accept error: {ex.Message}"); }
            }
        }

        private async Task HandleClientAsync(TcpClient client, CancellationToken ct)
        {
            try
            {
                using var stream = client.GetStream();
                // Read until \r\n\r\n (end of HTTP headers), max 8 KB
                var headerBuf = new byte[8192];
                int total = 0;
                while (total < headerBuf.Length)
                {
                    int n = await stream.ReadAsync(headerBuf.AsMemory(total, headerBuf.Length - total), ct);
                    if (n == 0) return;
                    total += n;
                    // Check for end of headers
                    int idx = IndexOf(headerBuf, total, "\r\n\r\n"u8);
                    if (idx >= 0) break;
                }

                string header = Encoding.ASCII.GetString(headerBuf, 0, total);
                string firstLine = header.Split('\n')[0].Trim();
                // "GET /path HTTP/1.1"
                string[] parts = firstLine.Split(' ');
                if (parts.Length < 2 || parts[0] != "GET")
                {
                    await SendResponseAsync(stream, 400, "text/plain", "Bad Request"u8.ToArray(), ct);
                    return;
                }

                string rawPath = parts[1];
                string path = Uri.UnescapeDataString(rawPath);

                // Separate query string
                string query = "";
                int qIdx = path.IndexOf('?');
                if (qIdx >= 0)
                {
                    query = path.Substring(qIdx + 1);
                    path = path.Substring(0, qIdx);
                }

                string[] segments = path.TrimStart('/').Split('/');

                if (segments.Length == 1 && segments[0] == "games")
                {
                    await ServeGameListAsync(stream, ct);
                }
                else if (segments.Length == 3 && segments[0] == "games" && segments[2] == "manifest")
                {
                    await ServeManifestAsync(stream, segments[1], ct);
                }
                else if (segments.Length == 3 && segments[0] == "games" && segments[2] == "metadata")
                {
                    await ServeMetadataAsync(stream, segments[1], ct);
                }
                else if (segments.Length == 3 && segments[0] == "games" && segments[2] == "cancel-check")
                {
                    await ServeCancelCheckAsync(stream, segments[1], ct);
                }
                else if (segments.Length == 3 && segments[0] == "games" && segments[2] == "cover")
                {
                    await ServeCoverAsync(stream, segments[1], ct);
                }
                else if (segments.Length == 3 && segments[0] == "games" && segments[2] == "hero")
                {
                    await ServeHeroAsync(stream, segments[1], ct);
                }
                else if (segments.Length >= 4 && segments[0] == "games" && segments[2] == "files")
                {
                    string relPath = string.Join("/", segments.Skip(3));

                    long totalBytesParam = 0;
                    string sessionIdParam = "";
                    if (!string.IsNullOrEmpty(query))
                    {
                        var qParts = query.Split('&');
                        foreach (var qp in qParts)
                        {
                            var kv = qp.Split('=');
                            if (kv.Length == 2)
                            {
                                if (kv[0] == "totalBytes" && long.TryParse(kv[1], out long tb))
                                {
                                    totalBytesParam = tb;
                                }
                                else if (kv[0] == "sessionId")
                                {
                                    sessionIdParam = kv[1];
                                }
                            }
                        }
                    }
                    if (string.IsNullOrEmpty(sessionIdParam))
                    {
                        sessionIdParam = $"fallback-{segments[1]}";
                    }

                    await ServeFileAsync(stream, segments[1], relPath, totalBytesParam, sessionIdParam, ct);
                }
                else
                {
                    await SendResponseAsync(stream, 404, "text/plain", "Not Found"u8.ToArray(), ct);
                }
            }
            catch (Exception ex)
            {
                App.Log($"LanShareServer client error: {ex.Message}");
            }
            finally
            {
                client.Dispose();
            }
        }

        private async Task ServeGameListAsync(NetworkStream stream, CancellationToken ct)
        {
            var names = _gameSource!()
                .Where(g => !string.IsNullOrEmpty(g.GameFolderPath) && Directory.Exists(g.GameFolderPath))
                .Select(g => g.GameName)
                .ToList();
            byte[] json = JsonSerializer.SerializeToUtf8Bytes(names, LanJsonContext.Default.ListString);
            await SendResponseAsync(stream, 200, "application/json", json, ct);
        }

        private async Task ServeMetadataAsync(NetworkStream stream, string gameName, CancellationToken ct)
        {
            var entry = _gameSource!().FirstOrDefault(g =>
                string.Equals(g.GameName, gameName, StringComparison.OrdinalIgnoreCase));

            if (entry == null)
            {
                await SendResponseAsync(stream, 404, "text/plain", "Game not found"u8.ToArray(), ct);
                return;
            }

            string relExePath = "";
            if (!string.IsNullOrEmpty(entry.ExePath) && !string.IsNullOrEmpty(entry.GameFolderPath))
            {
                try
                {
                    relExePath = Path.GetRelativePath(entry.GameFolderPath, entry.ExePath).Replace('\\', '/');
                }
                catch { }
            }

            var meta = new LanGameMetadata
            {
                GameName = entry.GameName,
                RunAsAdmin = entry.RunAsAdmin || RegistryHelper.GetCompatFlagRunAsAdmin(entry.ExePath),
                RelativeExePath = relExePath,
                InstallSizeMb = entry.InstallSizeMb
            };

            byte[] json = JsonSerializer.SerializeToUtf8Bytes(meta, LanJsonContext.Default.LanGameMetadata);
            await SendResponseAsync(stream, 200, "application/json", json, ct);
        }

        private async Task ServeCancelCheckAsync(NetworkStream stream, string gameName, CancellationToken ct)
        {
            bool cancelled;
            lock (_cancelledLock)
            {
                cancelled = _cancelledGames.Contains(gameName);
            }
            string response = cancelled ? "cancelled" : "active";
            await SendResponseAsync(stream, 200, "text/plain", Encoding.UTF8.GetBytes(response), ct);
        }

        private async Task ServeCoverAsync(NetworkStream stream, string gameName, CancellationToken ct)
        {
            var entry = _gameSource!().FirstOrDefault(g =>
                string.Equals(g.GameName, gameName, StringComparison.OrdinalIgnoreCase));

            if (entry == null || string.IsNullOrEmpty(entry.CoverImagePath) || !File.Exists(entry.CoverImagePath))
            {
                await SendResponseAsync(stream, 404, "text/plain", "Cover not found"u8.ToArray(), ct);
                return;
            }

            try
            {
                byte[] imgBytes = await File.ReadAllBytesAsync(entry.CoverImagePath, ct);
                string ext = Path.GetExtension(entry.CoverImagePath).ToLowerInvariant();
                string mimeType = ext switch
                {
                    ".png" => "image/png",
                    ".webp" => "image/webp",
                    ".gif" => "image/gif",
                    _ => "image/jpeg"
                };
                await SendResponseAsync(stream, 200, mimeType, imgBytes, ct);
            }
            catch (Exception ex)
            {
                App.Log($"Error serving cover: {ex.Message}");
                await SendResponseAsync(stream, 500, "text/plain", "Internal server error"u8.ToArray(), ct);
            }
        }

        private async Task ServeHeroAsync(NetworkStream stream, string gameName, CancellationToken ct)
        {
            var entry = _gameSource!().FirstOrDefault(g =>
                string.Equals(g.GameName, gameName, StringComparison.OrdinalIgnoreCase));

            if (entry == null || string.IsNullOrEmpty(entry.HeroImagePath) || !File.Exists(entry.HeroImagePath))
            {
                await SendResponseAsync(stream, 404, "text/plain", "Hero not found"u8.ToArray(), ct);
                return;
            }

            try
            {
                byte[] imgBytes = await File.ReadAllBytesAsync(entry.HeroImagePath, ct);
                string ext = Path.GetExtension(entry.HeroImagePath).ToLowerInvariant();
                string mimeType = ext switch
                {
                    ".png"  => "image/png",
                    ".webp" => "image/webp",
                    ".gif"  => "image/gif",
                    _       => "image/jpeg"
                };
                await SendResponseAsync(stream, 200, mimeType, imgBytes, ct);
            }
            catch (Exception ex)
            {
                App.Log($"Error serving hero: {ex.Message}");
                await SendResponseAsync(stream, 500, "text/plain", "Internal server error"u8.ToArray(), ct);
            }
        }

        private async Task ServeManifestAsync(NetworkStream stream, string gameName, CancellationToken ct)
        {
            lock (_cancelledLock)
            {
                _cancelledGames.Remove(gameName);
            }

            var entry = _gameSource!().FirstOrDefault(g =>
                string.Equals(g.GameName, gameName, StringComparison.OrdinalIgnoreCase));

            if (entry == null || string.IsNullOrEmpty(entry.GameFolderPath) || !Directory.Exists(entry.GameFolderPath))
            {
                await SendResponseAsync(stream, 404, "text/plain", "Game not found"u8.ToArray(), ct);
                return;
            }

            List<LanFileEntry> manifest;
            lock (_cacheLock)
            {
                if (!_manifestCache.TryGetValue(gameName, out manifest!))
                    manifest = null!;
            }

            if (manifest == null)
            {
                manifest = await BuildManifestAsync(entry.GameFolderPath, ct);
                lock (_cacheLock) _manifestCache[gameName] = manifest;
                OnManifestBuilt?.Invoke(gameName, manifest);
            }

            byte[] json = JsonSerializer.SerializeToUtf8Bytes(manifest, LanJsonContext.Default.ListLanFileEntry);
            await SendResponseAsync(stream, 200, "application/json", json, ct);
        }

        public static Task<List<LanFileEntry>> BuildManifestAsync(string folderPath, CancellationToken ct)
            => Task.Run(() =>
        {
            var entries = new List<LanFileEntry>();
            var files = Directory.GetFiles(folderPath, "*", SearchOption.AllDirectories);
            foreach (var file in files)
            {
                ct.ThrowIfCancellationRequested();
                string rel = Path.GetRelativePath(folderPath, file).Replace('\\', '/');
                var fi = new FileInfo(file);
                long size = fi.Length;
                long lastModified = new DateTimeOffset(fi.LastWriteTimeUtc).ToUnixTimeMilliseconds();
                entries.Add(new LanFileEntry { RelativePath = rel, Size = size, LastModified = lastModified });
            }
            return entries;
        });

        private async Task ServeFileAsync(NetworkStream stream, string gameName, string relPath, long totalBytesParam, string sessionIdParam, CancellationToken ct)
        {
            bool isCancelled;
            lock (_cancelledLock)
            {
                isCancelled = _cancelledGames.Contains(gameName);
            }
            if (isCancelled)
            {
                await SendResponseAsync(stream, 410, "text/plain", "Cancelled by host"u8.ToArray(), ct);
                return;
            }

            var entry = _gameSource!().FirstOrDefault(g =>
                string.Equals(g.GameName, gameName, StringComparison.OrdinalIgnoreCase));

            if (entry == null || string.IsNullOrEmpty(entry.GameFolderPath))
            {
                await SendResponseAsync(stream, 404, "text/plain", "Game not found"u8.ToArray(), ct);
                return;
            }

            // Prevent path traversal
            string safePath = relPath.Replace('/', Path.DirectorySeparatorChar);
            string fullPath = Path.GetFullPath(Path.Combine(entry.GameFolderPath, safePath));
            if (!fullPath.StartsWith(Path.GetFullPath(entry.GameFolderPath), StringComparison.OrdinalIgnoreCase))
            {
                await SendResponseAsync(stream, 403, "text/plain", "Forbidden"u8.ToArray(), ct);
                return;
            }

            if (!File.Exists(fullPath))
            {
                await SendResponseAsync(stream, 404, "text/plain", "File not found"u8.ToArray(), ct);
                return;
            }

            long fileSize = new FileInfo(fullPath).Length;
            string statusLine = "HTTP/1.1 200 OK\r\n";
            string headers = $"Content-Type: application/octet-stream\r\nContent-Length: {fileSize}\r\nConnection: close\r\n\r\n";

            long totalBytes = totalBytesParam > 0 ? totalBytesParam : fileSize;

            ActiveUpload? upload;
            lock (_uploadsLock)
            {
                upload = _activeUploads.FirstOrDefault(u => u.Guid == sessionIdParam);
                if (upload == null)
                {
                    var sessionCts = new CancellationTokenSource();
                    upload = new ActiveUpload(gameName, relPath, totalBytes, sessionCts)
                    {
                        Guid = sessionIdParam,
                        ActiveCount = 0
                    };
                    _activeUploads.Add(upload);
                }
                upload.ActiveCount++;
                upload.LastActive = DateTime.UtcNow;
                upload.RelativePath = relPath;
            }
            NotifyUploadsChanged();

            using var linkedCts = CancellationTokenSource.CreateLinkedTokenSource(ct, upload.Cts.Token);
            var linkedCt = linkedCts.Token;

            try
            {
                await stream.WriteAsync(Encoding.ASCII.GetBytes(statusLine + headers), linkedCt);

                using var fs = new FileStream(fullPath, FileMode.Open, FileAccess.Read, FileShare.Read,
                    BufferSize, FileOptions.SequentialScan | FileOptions.Asynchronous);
                var buf = new byte[BufferSize];
                int read;
                while ((read = await fs.ReadAsync(buf, linkedCt)) > 0)
                {
                    await stream.WriteAsync(buf.AsMemory(0, read), linkedCt);
                    lock (_uploadsLock)
                    {
                        upload.BytesSent += read;
                        upload.LastActive = DateTime.UtcNow;
                    }
                    NotifyUploadsChanged();
                }
            }
            finally
            {
                lock (_uploadsLock)
                {
                    upload.ActiveCount = Math.Max(0, upload.ActiveCount - 1);
                    upload.LastActive = DateTime.UtcNow;
                }
                linkedCts.Dispose();
                NotifyUploadsChanged();
            }
        }

        private static async Task SendResponseAsync(NetworkStream stream, int code, string contentType, byte[] body, CancellationToken ct)
        {
            string status = code == 200 ? "OK" : code == 404 ? "Not Found" : code == 403 ? "Forbidden" : "Bad Request";
            string header = $"HTTP/1.1 {code} {status}\r\nContent-Type: {contentType}\r\nContent-Length: {body.Length}\r\nConnection: close\r\n\r\n";
            await stream.WriteAsync(Encoding.ASCII.GetBytes(header), ct);
            await stream.WriteAsync(body, ct);
        }

        private static int IndexOf(byte[] haystack, int length, ReadOnlySpan<byte> needle)
        {
            for (int i = 0; i <= length - needle.Length; i++)
                if (haystack.AsSpan(i, needle.Length).SequenceEqual(needle))
                    return i;
            return -1;
        }

        // ── UDP discovery ──────────────────────────────────────────────────────

        private async Task UdpListenLoopAsync(CancellationToken ct)
        {
            while (!ct.IsCancellationRequested)
            {
                try
                {
                    var result = await _udpServer!.ReceiveAsync(ct);
                    string json = Encoding.UTF8.GetString(result.Buffer);

                    if (json.Contains("\"query\""))
                    {
                        await SendAnnounceAsync(result.RemoteEndPoint, ct);
                        PeerActivityDetected?.Invoke(this, EventArgs.Empty);
                    }
                    else if (json.Contains("\"announce\""))
                    {
                        var announce = JsonSerializer.Deserialize(json, LanJsonContext.Default.LanAnnounce);
                        if (announce != null && announce.DeviceId != _deviceId)
                        {
                            PeerActivityDetected?.Invoke(this, EventArgs.Empty);
                        }
                    }
                }
                catch (OperationCanceledException) { break; }
                catch { /* ignore malformed datagrams */ }
            }
        }

        private async Task AnnounceLoopAsync(CancellationToken ct)
        {
            while (!ct.IsCancellationRequested)
            {
                try
                {
                    await BroadcastAnnounceAsync(ct);
                    await Task.Delay(TimeSpan.FromSeconds(30), ct);
                }
                catch (OperationCanceledException) { break; }
                catch { }
            }
        }

        private async Task BroadcastAnnounceAsync(CancellationToken ct)
        {
            var announce = BuildAnnounce();
            byte[] data = JsonSerializer.SerializeToUtf8Bytes(announce, LanJsonContext.Default.LanAnnounce);
            using var udp = new UdpClient();
            udp.EnableBroadcast = true;
            await udp.SendAsync(data, new IPEndPoint(System.Net.IPAddress.Broadcast, _discoveryPort), ct);
        }

        private async Task SendAnnounceAsync(IPEndPoint target, CancellationToken ct)
        {
            var announce = BuildAnnounce();
            byte[] data = JsonSerializer.SerializeToUtf8Bytes(announce, LanJsonContext.Default.LanAnnounce);
            using var udp = new UdpClient();
            await udp.SendAsync(data, target, ct);
        }

        private LanAnnounce BuildAnnounce()
        {
            var games = _gameSource!()
                .Where(g => !string.IsNullOrEmpty(g.GameFolderPath) && Directory.Exists(g.GameFolderPath))
                .Select(g => g.GameName)
                .ToList();

            return new LanAnnounce
            {
                DeviceName = _deviceName,
                DeviceId   = _deviceId,
                Port       = _port,
                Games      = games
            };
        }

        public void Dispose() => Stop();
    }

    // ── Client ────────────────────────────────────────────────────────────────

    public class LanShareClient
    {
        private const int BufferSize    = 512 * 1024;
        private const int MaxParallel   = 4;

        private readonly string _deviceName;
        private readonly string _deviceId;

        public LanShareClient(string deviceName, string deviceId)
        {
            _deviceName = deviceName;
            _deviceId   = deviceId;
        }

        // Broadcast a query and collect peer announces for ~2 seconds.
        public async Task<List<LanPeer>> DiscoverPeersAsync(int discoveryPort, CancellationToken ct = default)
        {
            var peers = new Dictionary<string, LanPeer>(); // keyed by deviceId
            var udp   = new UdpClient(0); // bind to any port
            udp.EnableBroadcast = true;

            // Listen for incoming datagrams concurrently
            var listenTask = Task.Run(async () =>
            {
                var deadline = DateTime.UtcNow.AddSeconds(2.5);
                while (DateTime.UtcNow < deadline && !ct.IsCancellationRequested)
                {
                    udp.Client.ReceiveTimeout = 300;
                    try
                    {
                        var result = await udp.ReceiveAsync(ct);
                        var json = Encoding.UTF8.GetString(result.Buffer);
                        if (!json.Contains("\"announce\"")) continue;

                        var announce = JsonSerializer.Deserialize(json, LanJsonContext.Default.LanAnnounce);
                        if (announce == null || announce.DeviceId == _deviceId) continue; // skip self

                        lock (peers)
                        {
                            peers[announce.DeviceId] = new LanPeer
                            {
                                DeviceName = announce.DeviceName,
                                DeviceId   = announce.DeviceId,
                                IPAddress  = result.RemoteEndPoint.Address.ToString(),
                                Port       = announce.Port,
                                Games      = announce.Games
                            };
                        }
                    }
                    catch (OperationCanceledException) { break; }
                    catch { /* timeout or bad datagram */ }
                }
            }, ct);

            // Send query broadcast
            var query = new LanQuery();
            byte[] queryBytes = JsonSerializer.SerializeToUtf8Bytes(query, LanJsonContext.Default.LanQuery);
            await udp.SendAsync(queryBytes, new IPEndPoint(System.Net.IPAddress.Broadcast, discoveryPort), ct);

            await Task.WhenAny(listenTask, Task.Delay(2200, ct));
            udp.Dispose();

            lock (peers) return peers.Values.ToList();
        }

        public async Task<List<LanFileEntry>> GetManifestAsync(LanPeer peer, string gameName, CancellationToken ct = default)
        {
            using var http = MakeHttpClient(peer);
            string url = $"http://{peer.IPAddress}:{peer.Port}/games/{Uri.EscapeDataString(gameName)}/manifest";
            string json = await http.GetStringAsync(url, ct);
            return JsonSerializer.Deserialize(json, LanJsonContext.Default.ListLanFileEntry) ?? new();
        }

        public async Task<LanGameMetadata?> GetMetadataAsync(LanPeer peer, string gameName, CancellationToken ct = default)
        {
            try
            {
                using var http = MakeHttpClient(peer);
                string url = $"http://{peer.IPAddress}:{peer.Port}/games/{Uri.EscapeDataString(gameName)}/metadata";
                string json = await http.GetStringAsync(url, ct);
                return JsonSerializer.Deserialize(json, LanJsonContext.Default.LanGameMetadata);
            }
            catch (Exception ex)
            {
                App.Log($"GetMetadataAsync failed for '{gameName}': {ex.Message}");
                return null;
            }
        }

        public async Task DownloadGameAsync(
            LanPeer peer,
            string gameName,
            string destFolder,
            IProgress<LanDownloadProgress>? progress,
            CancellationToken ct)
        {
            string sessionId = System.Guid.NewGuid().ToString();
            progress?.Report(new LanDownloadProgress { Status = "Requesting game file list..." });

            var manifest = await GetManifestAsync(peer, gameName, ct);
            Directory.CreateDirectory(destFolder);

            // Determine which files need downloading (skip if local hash matches)
            var toDownload = new List<LanFileEntry>();
            long totalBytes = 0;

            int checkedCount = 0;
            foreach (var entry in manifest)
            {
                checkedCount++;
                progress?.Report(new LanDownloadProgress
                {
                    Status = $"Verifying existing files ({checkedCount}/{manifest.Count})...",
                    CurrentFile = entry.RelativePath,
                    BytesTransferred = checkedCount,
                    TotalBytes = manifest.Count
                });

                string localPath = Path.Combine(destFolder, entry.RelativePath.Replace('/', Path.DirectorySeparatorChar));
                bool skip = false;
                if (File.Exists(localPath))
                {
                    var fi = new FileInfo(localPath);
                    if (fi.Length == entry.Size)
                    {
                        long localModified = new DateTimeOffset(fi.LastWriteTimeUtc).ToUnixTimeMilliseconds();
                        // Allow a small tolerance of 2 seconds (2000 ms) for filesystem timestamp resolution differences
                        skip = Math.Abs(localModified - entry.LastModified) < 2000;
                    }
                }
                if (!skip)
                {
                    toDownload.Add(entry);
                    totalBytes += entry.Size;
                }
            }

            int filesCompleted = 0;
            long bytesTransferred = 0;
            var startTime = DateTime.UtcNow;
            var sem = new SemaphoreSlim(MaxParallel);

            var prog = new LanDownloadProgress
            {
                Status = "Downloading",
                TotalFiles = toDownload.Count,
                TotalBytes = totalBytes
            };

            using var queueCts = CancellationTokenSource.CreateLinkedTokenSource(ct);
            var queueCt = queueCts.Token;

            var tasks = toDownload.Select(async entry =>
            {
                try
                {
                    await sem.WaitAsync(queueCt);
                }
                catch (OperationCanceledException)
                {
                    return;
                }

                try
                {
                    queueCt.ThrowIfCancellationRequested();

                    prog.CurrentFile = entry.RelativePath;
                    progress?.Report(prog);

                    string localPath = Path.Combine(destFolder, entry.RelativePath.Replace('/', Path.DirectorySeparatorChar));
                    Directory.CreateDirectory(Path.GetDirectoryName(localPath)!);
                    string tmpPath = localPath + ".lanpart";

                    using var http = MakeHttpClient(peer);
                    string url = $"http://{peer.IPAddress}:{peer.Port}/games/{Uri.EscapeDataString(gameName)}/files/{Uri.EscapeDataString(entry.RelativePath)}?sessionId={sessionId}&totalBytes={totalBytes}";

                    using var response = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, queueCt);
                    response.EnsureSuccessStatusCode();

                    using var responseStream = await response.Content.ReadAsStreamAsync(queueCt);
                    using var fs = new FileStream(tmpPath, FileMode.Create, FileAccess.Write, FileShare.None,
                        BufferSize, FileOptions.Asynchronous);

                    var buf = new byte[BufferSize];
                    int read;
                    while ((read = await responseStream.ReadAsync(buf, queueCt)) > 0)
                    {
                        await fs.WriteAsync(buf.AsMemory(0, read), queueCt);
                        long newTotal = Interlocked.Add(ref bytesTransferred, read);
                        double elapsed = (DateTime.UtcNow - startTime).TotalSeconds;
                        prog.BytesTransferred = newTotal;
                        prog.SpeedBytesPerSec = elapsed > 0 ? newTotal / elapsed : 0;
                        progress?.Report(prog);
                    }

                    await fs.FlushAsync(queueCt);
                    fs.Dispose();

                    if (File.Exists(localPath)) File.Delete(localPath);
                    File.Move(tmpPath, localPath);

                    try
                    {
                        var destDateTime = DateTimeOffset.FromUnixTimeMilliseconds(entry.LastModified).UtcDateTime;
                        File.SetLastWriteTimeUtc(localPath, destDateTime);
                    }
                    catch (Exception ex)
                    {
                        App.Log($"Failed to set LastWriteTime on {localPath}: {ex.Message}");
                    }

                    prog.FilesCompleted = Interlocked.Increment(ref filesCompleted);
                    progress?.Report(prog);
                }
                catch (Exception)
                {
                    try { queueCts.Cancel(); } catch { }
                    throw;
                }
                finally
                {
                    sem.Release();
                }
            });

            try
            {
                await Task.WhenAll(tasks);
            }
            catch (Exception ex)
            {
                // Check if the host cancelled the download
                bool hostCancelled = false;
                try
                {
                    using var http = MakeHttpClient(peer);
                    string url = $"http://{peer.IPAddress}:{peer.Port}/games/{Uri.EscapeDataString(gameName)}/cancel-check";
                    string resp = await http.GetStringAsync(url, ct);
                    if (resp == "cancelled")
                    {
                        hostCancelled = true;
                    }
                }
                catch { }

                if (hostCancelled)
                {
                    throw new OperationCanceledException("Cancelled by host", ex);
                }

                throw;
            }
        }

        private static HttpClient MakeHttpClient(LanPeer peer)
        {
            var handler = new SocketsHttpHandler
            {
                ConnectTimeout = TimeSpan.FromSeconds(5),
                ResponseDrainTimeout = TimeSpan.FromMinutes(30),
                InitialHttp2StreamWindowSize = 256 * 1024
            };
            var client = new HttpClient(handler);
            client.Timeout = TimeSpan.FromMinutes(60);
            return client;
        }
    }
}
