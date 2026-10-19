using System.Buffers.Binary;
using System.Text;
using System.Text.Json;

// Development-only oracle. The protocol implementation is extracted unchanged
// from the existing WPF provider; this fixture never opens a pipe or a game.
static class ProviderProtocolFixture
{
    static byte[] Request(ushort op, ulong offset, uint length, byte[] path)
    {
        var result = new byte[24 + path.Length];
        BinaryPrimitives.WriteUInt32LittleEndian(result, 0x31504232);
        BinaryPrimitives.WriteUInt16LittleEndian(result.AsSpan(4), 1);
        BinaryPrimitives.WriteUInt16LittleEndian(result.AsSpan(6), op);
        BinaryPrimitives.WriteUInt64LittleEndian(result.AsSpan(8), offset);
        BinaryPrimitives.WriteUInt32LittleEndian(result.AsSpan(16), length);
        BinaryPrimitives.WriteUInt32LittleEndian(result.AsSpan(20), (uint)path.Length);
        path.CopyTo(result, 24);
        return result;
    }
    static object Inspect(byte[] bytes)
    {
        try
        {
            var parsed = BinaryProviderProtocol.ParseRequest(bytes);
            return new { bytes = Convert.ToHexString(bytes), ok = true, operation = parsed.Operation,
                offset = parsed.Offset.ToString(System.Globalization.CultureInfo.InvariantCulture),
                length = parsed.Length, path = parsed.LogicalPath };
        }
        catch(Exception error)
        {
            return new { bytes = Convert.ToHexString(bytes), ok = false, status = BinaryProviderProtocol.Classify(error) };
        }
    }
    public static int Main()
    {
        var requests = new List<object>();
        foreach(var path in new [] { "char/sig/png10000.iff", "CHAR\\SIG\\PNG10000.IFF", "测试/球.iff", "", " ", "../escape.iff", "/absolute", "x\0.iff", new string('a',260), new string('a',261), new string('é',130), new string('é',131) })
            foreach(ushort op in new ushort[]{1,2,0,3})
                foreach(ulong offset in new ulong[]{0,1,ulong.MaxValue})
                    foreach(uint length in new uint[]{0,1,1048576,1048577})
                        requests.Add(Inspect(Request(op, offset, length, Encoding.UTF8.GetBytes(path))));
        var valid = Request(2,123,4096,Encoding.UTF8.GetBytes("char/sig/png10000.iff"));
        for(var length=0; length<valid.Length; length++) requests.Add(Inspect(valid[..length]));
        for(var at=0; at<24; at++)
        {
            var changed = (byte[])valid.Clone(); changed[at] ^= 0x80;
            requests.Add(Inspect(changed));
        }
        foreach(var path in new byte[][] { new byte[]{0x80}, new byte[]{0xC0,0xAF},new byte[]{0xED,0xA0,0x80},new byte[]{0xF4,0x90,0x80,0x80} })
            requests.Add(Inspect(Request(2,0,1,path)));
        requests.Add(Inspect(valid.Concat(new byte[]{0}).ToArray()));
        var responses = new List<object>();
        foreach(ushort op in new ushort[]{1,2})
            foreach(ulong size in new ulong[]{0,9000,ulong.MaxValue})
                foreach(var count in new[]{0,1,257})
                {
                    var payload = Enumerable.Range(0,count).Select(i=>(byte)(i%251)).ToArray();
                    responses.Add(new { operation=op, size=size.ToString(System.Globalization.CultureInfo.InvariantCulture), payload=Convert.ToHexString(payload), bytes=Convert.ToHexString(BinaryProviderProtocol.EncodeSuccess(op,size,payload)) });
                }
        var errors = new uint[]{0,1,2,3,4,99,uint.MaxValue}.Select(status=> new {status,bytes=Convert.ToHexString(BinaryProviderProtocol.EncodeError(status))}).ToArray();
        Console.WriteLine(JsonSerializer.Serialize(new {schema="nba2k27-provider-wire-oracle-v1",requests,responses,errors,gameAccess=false}));
        return 0;
    }
}
