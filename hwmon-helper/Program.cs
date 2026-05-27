using System.Text.Json;
using HwmonHelper;

const string Version = "0.1.0";
var stdin = Console.In;
var stdout = Console.Out;
var stderr = Console.Error;

using var hw = new HardwareReader();
hw.DescribeToStderr(stderr);

string? line;
while ((line = stdin.ReadLine()) is not null)
{
    if (string.IsNullOrWhiteSpace(line)) continue;

    using var doc = TryParse(line);
    if (doc is null)
    {
        WriteError("invalid_json");
        continue;
    }

    if (!doc.RootElement.TryGetProperty("cmd", out var cmdProp) ||
        cmdProp.ValueKind != JsonValueKind.String)
    {
        WriteError("missing_cmd");
        continue;
    }

    switch (cmdProp.GetString())
    {
        case "hello":
            Write(new HelloResponse(Version, new[] { "hello", "poll" }),
                  ProtocolJsonContext.Default.HelloResponse);
            break;
        case "poll":
            Write(hw.Snapshot(), ProtocolJsonContext.Default.PollResponse);
            break;
        case "shutdown":
            return 0;
        default:
            WriteError("unknown_cmd");
            break;
    }
}

return 0;

static JsonDocument? TryParse(string s)
{
    try { return JsonDocument.Parse(s); } catch { return null; }
}

void Write<T>(T value, System.Text.Json.Serialization.Metadata.JsonTypeInfo<T> ti)
{
    stdout.WriteLine(JsonSerializer.Serialize(value, ti));
    stdout.Flush();
}

void WriteError(string msg)
{
    Write(new ErrorResponse(msg), ProtocolJsonContext.Default.ErrorResponse);
}
