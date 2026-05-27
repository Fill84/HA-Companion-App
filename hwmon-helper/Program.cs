using System.Text.Json;
using HwmonHelper;

const string Version = "0.1.0";
var stdin = Console.In;
var stdout = Console.Out;

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
            Write(new HelloResponse(Version, new[] { "hello" }),
                  ProtocolJsonContext.Default.HelloResponse);
            break;
        case "poll":
            // Phase 2 task 3 implements the real reading. For task 1 we
            // return nulls so the Rust client can be tested end-to-end first.
            Write(new PollResponse(null, null),
                  ProtocolJsonContext.Default.PollResponse);
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
