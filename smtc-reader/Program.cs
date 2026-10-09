using System.Text.Json;
using System.Text.Json.Serialization;
using Windows.Media.Control;
using Windows.Storage.Streams;

// ── Main entry point ─────────────────────────────────────────────────────────
try
{
    var manager = await GlobalSystemMediaTransportControlsSessionManager.RequestAsync();

    // Check for control commands (playpause, play, pause, next, previous)
    if (args.Length > 0)
    {
        var cmd = args[0].ToLower();
        var targetSession = manager.GetCurrentSession() ?? manager.GetSessions().FirstOrDefault();
        if (targetSession != null)
        {
            if (cmd == "playpause") await targetSession.TryTogglePlayPauseAsync();
            else if (cmd == "play") await targetSession.TryPlayAsync();
            else if (cmd == "pause") await targetSession.TryPauseAsync();
            else if (cmd == "next") await targetSession.TrySkipNextAsync();
            else if (cmd == "previous") await targetSession.TrySkipPreviousAsync();
        }
        return;
    }

    var sessions = manager.GetSessions();
    var results = new List<MediaSession>();

    foreach (var session in sessions)
    {
        try
        {
            var props = await session.TryGetMediaPropertiesAsync();
            if (props == null) continue;

            var playback = session.GetPlaybackInfo();
            var timeline = session.GetTimelineProperties();
            var appId = session.SourceAppUserModelId ?? "";

            var status = playback.PlaybackStatus switch
            {
                GlobalSystemMediaTransportControlsSessionPlaybackStatus.Playing => "playing",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus.Paused  => "paused",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus.Stopped => "stopped",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus.Changing => "changing",
                _ => "unknown"
            };

            long duration = 0, position = 0;
            try { duration = (long)timeline.EndTime.TotalSeconds; } catch { }
            try { position = (long)timeline.Position.TotalSeconds; } catch { }

            // Read thumbnail as base64 data URI
            string? thumbnail = null;
            try
            {
                if (props.Thumbnail != null)
                {
                    var stream = await props.Thumbnail.OpenReadAsync();
                    if (stream.Size > 0)
                    {
                        var reader = new DataReader(stream.GetInputStreamAt(0));
                        await reader.LoadAsync((uint)stream.Size);
                        var bytes = new byte[stream.Size];
                        reader.ReadBytes(bytes);
                        var mime = string.IsNullOrWhiteSpace(stream.ContentType) 
                            ? "image/jpeg" 
                            : stream.ContentType;
                        thumbnail = $"data:{mime};base64,{Convert.ToBase64String(bytes)}";
                    }
                }
            }
            catch { /* thumbnail optional */ }

            results.Add(new MediaSession
            {
                Title     = props.Title ?? "",
                Artist    = props.Artist ?? "",
                Album     = props.AlbumTitle ?? "",
                AppId     = appId,
                Status    = status,
                Duration  = duration,
                Position  = position,
                Thumbnail = thumbnail,
            });
        }
        catch { /* skip failing sessions */ }
    }

    var output = new OutputPayload { Sessions = results };
    Console.WriteLine(JsonSerializer.Serialize(output, OutputPayloadContext.Default.OutputPayload));
}
catch (Exception ex)
{
    var error = new OutputPayload { Error = ex.Message };
    Console.WriteLine(JsonSerializer.Serialize(error, OutputPayloadContext.Default.OutputPayload));
}

// ── Models ───────────────────────────────────────────────────────────────────

public class MediaSession
{
    [JsonPropertyName("title")]    public string  Title     { get; set; } = "";
    [JsonPropertyName("artist")]   public string  Artist    { get; set; } = "";
    [JsonPropertyName("album")]    public string  Album     { get; set; } = "";
    [JsonPropertyName("appId")]    public string  AppId     { get; set; } = "";
    [JsonPropertyName("status")]   public string  Status    { get; set; } = "unknown";
    [JsonPropertyName("duration")] public long    Duration  { get; set; }
    [JsonPropertyName("position")] public long    Position  { get; set; }
    [JsonPropertyName("thumbnail")] public string? Thumbnail { get; set; }
}

public class OutputPayload
{
    [JsonPropertyName("sessions")] public List<MediaSession> Sessions { get; set; } = new();
    [JsonPropertyName("error")]    public string?            Error    { get; set; }
}

[JsonSerializable(typeof(OutputPayload))]
[JsonSerializable(typeof(MediaSession))]
[JsonSourceGenerationOptions(WriteIndented = false)]
internal partial class OutputPayloadContext : JsonSerializerContext { }
