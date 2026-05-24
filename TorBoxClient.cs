using System;
using System.Collections.Generic;
using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;

namespace LudusaviWrap
{
    public class TorBoxTorrent
    {
        [JsonPropertyName("id")]
        public int Id { get; set; }

        [JsonPropertyName("name")]
        public string Name { get; set; } = "";

        [JsonPropertyName("download_state")]
        public string DownloadState { get; set; } = "";

        [JsonPropertyName("progress")]
        public double Progress { get; set; }

        [JsonPropertyName("size")]
        public long Size { get; set; }

        [JsonPropertyName("cached")]
        public bool Cached { get; set; }

        [JsonPropertyName("files")]
        public List<TorBoxFile>? Files { get; set; }
    }

    public class TorBoxFile
    {
        [JsonPropertyName("id")]
        public int Id { get; set; }

        [JsonPropertyName("name")]
        public string Name { get; set; } = "";

        [JsonPropertyName("size")]
        public long Size { get; set; }

        [JsonPropertyName("short_name")]
        public string ShortName { get; set; } = "";
    }

    public class TorBoxAddData
    {
        [JsonPropertyName("torrent_id")]
        public int TorrentId { get; set; }

        [JsonPropertyName("name")]
        public string Name { get; set; } = "";

        [JsonPropertyName("hash")]
        public string Hash { get; set; } = "";
    }

    public class TorBoxAddResponse
    {
        [JsonPropertyName("success")]
        public bool Success { get; set; }

        [JsonPropertyName("detail")]
        public string? Detail { get; set; }

        [JsonPropertyName("error")]
        public string? Error { get; set; }

        [JsonPropertyName("data")]
        public TorBoxAddData? Data { get; set; }
    }

    public class TorBoxListResponse
    {
        [JsonPropertyName("success")]
        public bool Success { get; set; }

        [JsonPropertyName("detail")]
        public string? Detail { get; set; }

        [JsonPropertyName("data")]
        public List<TorBoxTorrent>? Data { get; set; }
    }

    public class TorBoxSingleResponse
    {
        [JsonPropertyName("success")]
        public bool Success { get; set; }

        [JsonPropertyName("detail")]
        public string? Detail { get; set; }

        [JsonPropertyName("data")]
        public TorBoxTorrent? Data { get; set; }
    }

    public class TorBoxLinkResponse
    {
        [JsonPropertyName("success")]
        public bool Success { get; set; }

        [JsonPropertyName("detail")]
        public string? Detail { get; set; }

        [JsonPropertyName("error")]
        public string? Error { get; set; }

        [JsonPropertyName("data")]
        public string? Data { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = false)]
    [JsonSerializable(typeof(TorBoxAddResponse))]
    [JsonSerializable(typeof(TorBoxListResponse))]
    [JsonSerializable(typeof(TorBoxSingleResponse))]
    [JsonSerializable(typeof(TorBoxLinkResponse))]
    internal partial class TorBoxSourceGenerationContext : JsonSerializerContext { }

    public class TorBoxClient
    {
        private static readonly HttpClient HttpClient = new() { Timeout = TimeSpan.FromSeconds(60) };
        private const string BaseUrl = "https://api.torbox.app/v1/api";
        private readonly string _apiKey;

        public TorBoxClient(string apiKey)
        {
            _apiKey = apiKey;
        }

        private HttpRequestMessage CreateRequest(HttpMethod method, string path)
        {
            var req = new HttpRequestMessage(method, $"{BaseUrl}{path}");
            req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _apiKey);
            return req;
        }

        public async Task<int> AddMagnetAsync(string magnetUri)
        {
            using var req = CreateRequest(HttpMethod.Post, "/torrents/createtorrent");
            req.Content = new FormUrlEncodedContent(
            [
                new KeyValuePair<string, string>("magnet", magnetUri)
            ]);

            using var resp = await HttpClient.SendAsync(req);
            if (!resp.IsSuccessStatusCode)
            {
                string errorContent = await resp.Content.ReadAsStringAsync();
                string? detail = null;
                try
                {
                    var errRes = JsonSerializer.Deserialize(errorContent, TorBoxSourceGenerationContext.Default.TorBoxAddResponse);
                    detail = errRes?.Error ?? errRes?.Detail;
                }
                catch { }
                throw new Exception(detail ?? $"HTTP {(int)resp.StatusCode} ({resp.StatusCode}): {errorContent}");
            }
            string json = await resp.Content.ReadAsStringAsync();
            var result = JsonSerializer.Deserialize(json, TorBoxSourceGenerationContext.Default.TorBoxAddResponse);

            if (result?.Data == null)
                throw new Exception(result?.Error ?? result?.Detail ?? "Failed to add torrent to TorBox");

            return result.Data.TorrentId;
        }

        public async Task<TorBoxTorrent?> GetTorrentInfoAsync(int torrentId)
        {
            using var req = CreateRequest(HttpMethod.Get, $"/torrents/mylist?id={torrentId}&bypass_cache=true");
            using var resp = await HttpClient.SendAsync(req);
            if (!resp.IsSuccessStatusCode)
            {
                string errorContent = await resp.Content.ReadAsStringAsync();
                string? detail = null;
                try
                {
                    var errRes = JsonSerializer.Deserialize(errorContent, TorBoxSourceGenerationContext.Default.TorBoxSingleResponse);
                    detail = errRes?.Detail;
                }
                catch { }
                throw new Exception(detail ?? $"HTTP {(int)resp.StatusCode} ({resp.StatusCode}): {errorContent}");
            }
            string json = await resp.Content.ReadAsStringAsync();
            var result = JsonSerializer.Deserialize(json, TorBoxSourceGenerationContext.Default.TorBoxSingleResponse);
            return result?.Data;
        }

        public async Task<string> RequestDownloadLinkAsync(int torrentId, int fileId)
        {
            using var req = CreateRequest(HttpMethod.Get, $"/torrents/requestdl?token={Uri.EscapeDataString(_apiKey)}&torrent_id={torrentId}&file_id={fileId}");

            using var resp = await HttpClient.SendAsync(req);
            if (!resp.IsSuccessStatusCode)
            {
                string errorContent = await resp.Content.ReadAsStringAsync();
                string? detail = null;
                try
                {
                    var errRes = JsonSerializer.Deserialize(errorContent, TorBoxSourceGenerationContext.Default.TorBoxLinkResponse);
                    detail = errRes?.Error ?? errRes?.Detail;
                }
                catch { }
                throw new Exception(detail ?? $"HTTP {(int)resp.StatusCode} ({resp.StatusCode}): {errorContent}");
            }
            string json = await resp.Content.ReadAsStringAsync();
            var result = JsonSerializer.Deserialize(json, TorBoxSourceGenerationContext.Default.TorBoxLinkResponse);

            if (string.IsNullOrEmpty(result?.Data))
                throw new Exception(result?.Error ?? result?.Detail ?? "Failed to get download link from TorBox");

            return result.Data;
        }

        public async Task DownloadFileAsync(
            string url,
            string destPath,
            IProgress<(long bytesDownloaded, long totalBytes)>? progress,
            CancellationToken ct)
        {
            using var req = new HttpRequestMessage(HttpMethod.Get, url);
            using var resp = await HttpClient.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, ct);
            resp.EnsureSuccessStatusCode();

            long total = resp.Content.Headers.ContentLength ?? -1;
            long downloaded = 0;

            Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);

            await using var stream = await resp.Content.ReadAsStreamAsync(ct);
            await using var file = File.Create(destPath);

            byte[] buffer = new byte[81920];
            int bytesRead;
            while ((bytesRead = await stream.ReadAsync(buffer, ct)) > 0)
            {
                await file.WriteAsync(buffer.AsMemory(0, bytesRead), ct);
                downloaded += bytesRead;
                progress?.Report((downloaded, total));
            }
        }
    }
}
