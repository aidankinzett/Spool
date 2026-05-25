using System;
using System.Collections.Generic;
using System.Linq;
using System.Net;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;

namespace LudusaviWrap
{
    public class LockStatusResponse
    {
        [JsonPropertyName("locked")]
        public bool Locked { get; set; }

        [JsonPropertyName("device_id")]
        public string? DeviceId { get; set; }

        [JsonPropertyName("device_name")]
        public string? DeviceName { get; set; }

        [JsonPropertyName("stale")]
        public bool Stale { get; set; }
    }

    public class AcquireRequest
    {
        [JsonPropertyName("device_id")]
        public string DeviceId { get; set; } = "";

        [JsonPropertyName("device_name")]
        public string DeviceName { get; set; } = "";
    }

    public class AcquireConflictResponse
    {
        [JsonPropertyName("device_name")]
        public string? DeviceName { get; set; }
    }

    public class LatestBackupResponse
    {
        [JsonPropertyName("found")]
        public bool Found { get; set; }

        [JsonPropertyName("device_id")]
        public string? DeviceId { get; set; }

        [JsonPropertyName("device_name")]
        public string? DeviceName { get; set; }

        [JsonPropertyName("occurred_at")]
        public string? OccurredAt { get; set; }
    }

    public class HealthResponse
    {
        [JsonPropertyName("ok")]
        public bool Ok { get; set; }

        [JsonPropertyName("version")]
        public string? Version { get; set; }
    }

    public class RegisterRequest
    {
        [JsonPropertyName("username")]
        public string Username { get; set; } = "";
    }

    public class RegisterResponse
    {
        [JsonPropertyName("api_key")]
        public string? ApiKey { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = false)]
    [JsonSerializable(typeof(LockStatusResponse))]
    [JsonSerializable(typeof(AcquireRequest))]
    [JsonSerializable(typeof(AcquireConflictResponse))]
    [JsonSerializable(typeof(RegisterRequest))]
    [JsonSerializable(typeof(RegisterResponse))]
    [JsonSerializable(typeof(HealthResponse))]
    [JsonSerializable(typeof(LatestBackupResponse))]
    internal partial class LockSourceGenerationContext : JsonSerializerContext
    {
    }

    public enum AcquireOutcome { Acquired, Conflict, ServerError, Unavailable }

    public class AcquireResult
    {
        public AcquireOutcome Outcome { get; init; }
        public string? ConflictDeviceName { get; init; }

        public static readonly AcquireResult Acquired  = new() { Outcome = AcquireOutcome.Acquired };
        public static readonly AcquireResult ServerError  = new() { Outcome = AcquireOutcome.ServerError };
        public static readonly AcquireResult Unavailable  = new() { Outcome = AcquireOutcome.Unavailable };

        public static AcquireResult ConflictWith(string? deviceName) =>
            new() { Outcome = AcquireOutcome.Conflict, ConflictDeviceName = deviceName };
    }

    public class PlayStateLockClient
    {
        private static readonly HttpClient HttpClient = new() { Timeout = TimeSpan.FromSeconds(10) };

        private readonly string _baseUrl;
        private readonly string _apiKey;
        private readonly string _deviceId;
        private readonly string _deviceName;

        public PlayStateLockClient(string serverUrl, string apiKey, string deviceId, string deviceName)
        {
            _baseUrl = serverUrl.TrimEnd('/');
            _apiKey = apiKey;
            _deviceId = deviceId;
            _deviceName = deviceName;
        }

        private HttpRequestMessage BuildRequest(HttpMethod method, string path)
        {
            var req = new HttpRequestMessage(method, _baseUrl + path);
            req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _apiKey);
            req.Headers.Add("X-Device-Id", _deviceId);
            return req;
        }

        public async Task<LockStatusResponse?> CheckLockAsync(string gameName)
        {
            try
            {
                using var req = BuildRequest(HttpMethod.Get, $"/locks/{Uri.EscapeDataString(gameName)}");
                using var resp = await HttpClient.SendAsync(req);
                if (!resp.IsSuccessStatusCode) return null;

                string json = await resp.Content.ReadAsStringAsync();
                return JsonSerializer.Deserialize(json, LockSourceGenerationContext.Default.LockStatusResponse);
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or JsonException)
            {
                return null;
            }
        }

        public async Task<AcquireResult> AcquireLockAsync(string gameName)
        {
            try
            {
                using var req = BuildRequest(HttpMethod.Post, $"/locks/{Uri.EscapeDataString(gameName)}/acquire");
                var body = new AcquireRequest { DeviceId = _deviceId, DeviceName = _deviceName };
                req.Content = JsonContent.Create(body, LockSourceGenerationContext.Default.AcquireRequest);

                using var resp = await HttpClient.SendAsync(req);

                if (resp.IsSuccessStatusCode)
                    return AcquireResult.Acquired;

                if ((int)resp.StatusCode == 409)
                {
                    string json = await resp.Content.ReadAsStringAsync();
                    var conflict = JsonSerializer.Deserialize(json, LockSourceGenerationContext.Default.AcquireConflictResponse);
                    return AcquireResult.ConflictWith(conflict?.DeviceName);
                }

                return AcquireResult.ServerError;
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or JsonException)
            {
                return AcquireResult.Unavailable;
            }
        }

        public async Task ReleaseLockAsync(string gameName)
        {
            try
            {
                using var req = BuildRequest(HttpMethod.Post, $"/locks/{Uri.EscapeDataString(gameName)}/release");
                await HttpClient.SendAsync(req);
            }
            catch
            {
                // Fire-and-forget; swallow all errors
            }
        }

        public async Task<LatestBackupResponse?> GetLatestBackupEventAsync(string gameName)
        {
            try
            {
                using var req = BuildRequest(HttpMethod.Get, $"/events/{Uri.EscapeDataString(gameName)}/latest-backup");
                using var resp = await HttpClient.SendAsync(req);
                if (!resp.IsSuccessStatusCode) return null;
                var json = await resp.Content.ReadAsStringAsync();
                return JsonSerializer.Deserialize(json, LockSourceGenerationContext.Default.LatestBackupResponse);
            }
            catch (Exception ex)
            {
                App.Log($"GetLatestBackupEventAsync failed for '{gameName}': {ex.Message}");
                return null;
            }
        }

        private HttpRequestMessage BuildEventRequest(string path)
        {
            var req = BuildRequest(HttpMethod.Post, path);
            req.Headers.Add("X-Device-Name", _deviceName);
            return req;
        }

        public async Task RecordBackupAsync(string gameName)
        {
            try
            {
                using var req = BuildEventRequest($"/events/{Uri.EscapeDataString(gameName)}/backup");
                await HttpClient.SendAsync(req);
            }
            catch (Exception ex)
            {
                App.Log($"RecordBackupAsync failed for '{gameName}': {ex.Message}");
            }
        }

        public async Task RecordRestoreAsync(string gameName)
        {
            try
            {
                using var req = BuildEventRequest($"/events/{Uri.EscapeDataString(gameName)}/restore");
                await HttpClient.SendAsync(req);
            }
            catch (Exception ex)
            {
                App.Log($"RecordRestoreAsync failed for '{gameName}': {ex.Message}");
            }
        }

        public async Task StartHeartbeatLoopAsync(string gameName, CancellationToken ct)
        {
            try
            {
                while (!ct.IsCancellationRequested)
                {
                    await Task.Delay(TimeSpan.FromSeconds(60), ct);

                    using var req = BuildRequest(HttpMethod.Post, $"/locks/{Uri.EscapeDataString(gameName)}/heartbeat");
                    try
                    {
                        await HttpClient.SendAsync(req, ct);
                    }
                    catch
                    {
                        // Ignore individual heartbeat failures; keep looping
                    }
                }
            }
            catch (OperationCanceledException)
            {
                // Normal shutdown
            }
        }

        // Fetches the server's health and version. No auth required.
        public static async Task<HealthResponse?> CheckHealthAsync(string serverUrl)
        {
            try
            {
                using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(5) };
                var resp = await http.GetAsync(serverUrl.TrimEnd('/') + "/health");
                if (!resp.IsSuccessStatusCode) return null;
                var json = await resp.Content.ReadAsStringAsync();
                return JsonSerializer.Deserialize(json, LockSourceGenerationContext.Default.HealthResponse);
            }
            catch
            {
                return null;
            }
        }

        // Probes the LAN for a running spool server.
        // Tries the well-known mDNS hostname plus every address in the local /24 subnet.
        // Returns URLs of all responding servers (typically just one).
        public static async Task<List<string>> ScanLanAsync()
        {
            var candidates = new List<string> { "http://spool-lock.local:47633" };

            try
            {
                var hostEntry = Dns.GetHostEntry(Dns.GetHostName());
                var localIp = hostEntry.AddressList
                    .FirstOrDefault(a => a.AddressFamily == AddressFamily.InterNetwork
                                      && !IPAddress.IsLoopback(a));
                if (localIp != null)
                {
                    var bytes = localIp.GetAddressBytes();
                    var subnet = $"{bytes[0]}.{bytes[1]}.{bytes[2]}";
                    for (int i = 1; i <= 254; i++)
                        candidates.Add($"http://{subnet}.{i}:47633");
                }
            }
            catch { }

            using var http = new HttpClient { Timeout = TimeSpan.FromMilliseconds(700) };
            var tasks = candidates.Select(async url =>
            {
                try
                {
                    var resp = await http.GetAsync(url + "/health");
                    if (resp.IsSuccessStatusCode)
                    {
                        var body = await resp.Content.ReadAsStringAsync();
                        return body.Contains("\"ok\"") ? url : null;
                    }
                }
                catch { }
                return null;
            });

            var results = await Task.WhenAll(tasks);
            return results.Where(r => r != null).Select(r => r!).Distinct().ToList();
        }

        // Registers a new account on the server using the admin secret.
        // Returns the generated API key on success, null on failure.
        // errorMessage is set to a human-readable reason on failure.
        public static async Task<(string? ApiKey, string? Error)> RegisterAsync(
            string serverUrl, string adminSecret, string username)
        {
            try
            {
                using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(10) };
                using var req = new HttpRequestMessage(HttpMethod.Post, serverUrl.TrimEnd('/') + "/auth/register");
                req.Headers.Add("X-Admin-Secret", adminSecret);
                var body = new RegisterRequest { Username = username };
                req.Content = JsonContent.Create(body, LockSourceGenerationContext.Default.RegisterRequest);

                using var resp = await http.SendAsync(req);
                var json = await resp.Content.ReadAsStringAsync();

                if (resp.IsSuccessStatusCode)
                {
                    var parsed = JsonSerializer.Deserialize(json, LockSourceGenerationContext.Default.RegisterResponse);
                    return parsed?.ApiKey is { Length: > 0 } key
                        ? (key, null)
                        : (null, "Server returned an empty API key.");
                }

                if ((int)resp.StatusCode == 409)
                    return (null, "That username is already taken.");
                if ((int)resp.StatusCode == 403)
                    return (null, "Admin secret is incorrect.");

                return (null, $"Server returned {(int)resp.StatusCode}.");
            }
            catch (HttpRequestException)
            {
                return (null, "Could not reach the server. Check the URL and try again.");
            }
            catch (TaskCanceledException)
            {
                return (null, "Request timed out.");
            }
        }
    }
}
