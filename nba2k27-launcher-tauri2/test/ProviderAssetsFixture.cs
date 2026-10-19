using System.Text.Json;

// Original ProviderService, synthetic profile files, JSON-only operations.
// No named-pipe client, game identity lookup or loader is invoked.
static class ProviderAssetsFixture
{
    static int[][] OrdinalTable()
    {
        // Ask the actual WPF runtime comparer, not Windows NLS or an unrelated
        // full-uppercase algorithm. Those disagree on final sigma and long S.
        var classes=new Dictionary<string,int>(StringComparer.OrdinalIgnoreCase);
        var folds=new List<int[]>();
        for(var scalar=0;scalar<=0x10FFFF;scalar++)
        {
            if(scalar>=0xD800&&scalar<=0xDFFF)continue;
            var s=char.ConvertFromUtf32(scalar);
            if(classes.TryGetValue(s,out var representative))folds.Add(new[]{scalar,representative});
            else classes.Add(s,scalar);
        }
        return folds.ToArray();
    }
    const string Token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    static readonly object[] Files = new object[] {
        new {path="char/sig/png10000.iff",data="00010203FDFEFF",count=1297},
        new {path="balls/happy.iff",data="102030",count=23},
        new {path="empty.iff",data="",count=0},
        new {path="hidden.iff",data="424344",count=3},
        new {path="mutable/existing.iff",data="A0B0C0",count=7},
        new {path="unicode/é.iff",data="01",count=1},
        new {path="unicode/ß.iff",data="02",count=1},
        new {path="unicode/ı.iff",data="03",count=1},
        new {path="unicode/ſ.iff",data="04",count=1},
        new {path="unicode/Σ.iff",data="05",count=1},
        new {path="unicode/𐐀.iff",data="06",count=1},
    };
    static object Step(string kind,string path,ulong offset=0,uint length=0,string? source=null) => new {kind,path,offset=offset.ToString(System.Globalization.CultureInfo.InvariantCulture),length,source};
    static void WriteFile(string root,string relative,string hex,int length)
    {
        var file=Path.Combine(root,relative.Replace('/',Path.DirectorySeparatorChar));
        Directory.CreateDirectory(Path.GetDirectoryName(file)!);
        var seed=Convert.FromHexString(hex);
        File.WriteAllBytes(file,Enumerable.Range(0,length).Select(i=>seed[i%seed.Length]).ToArray());
    }
    static async Task<JsonElement> Dispatch(ProviderService service,object request)
    {
        using var document=JsonDocument.Parse(JsonSerializer.Serialize(request));
        var result=await service.DispatchAsync(document.RootElement);
        return JsonSerializer.SerializeToElement(result,JsonConfig.Options);
    }
    static async Task<object> Run(ProviderService service,string root,object spec)
    {
        var s=JsonSerializer.SerializeToElement(spec);
        var kind=s.GetProperty("kind").GetString(); var path=s.GetProperty("path").GetString();
        try
        {
            object expected;
            if(kind=="query")
            {
                var r=await Dispatch(service,new {sessionToken=Token,operation="query",logicalPath=path});
                expected=new {logicalPath=r.GetProperty("logicalPath").GetString(),size=r.GetProperty("size").GetInt64()};
            }
            else if(kind=="read")
            {
                var opened=await Dispatch(service,new {sessionToken=Token,operation="open",logicalPath=path});
                var handle=opened.GetProperty("handle").GetString();
                try
                {
                    var r=await Dispatch(service,new {sessionToken=Token,operation="read",handle,length=s.GetProperty("length").GetUInt32(),offset=ulong.Parse(s.GetProperty("offset").GetString()!)});
                    expected=new {logicalPath=r.GetProperty("logicalPath").GetString(),size=r.GetProperty("size").GetInt64(),data=Convert.ToHexString(Convert.FromBase64String(r.GetProperty("data").GetString()!))};
                }
                finally {await Dispatch(service,new {sessionToken=Token,operation="close",handle});}
            }
            else if(kind=="alias")
            {
                var source=s.GetProperty("source").GetString();
                var r=await Dispatch(service,new {sessionToken=Token,operation="set-alias",logicalPath=path,restore=source is null,sourceLogicalPath=source});
                expected=new {logicalPath=r.GetProperty("logicalPath").GetString(),size=r.GetProperty("size").GetInt64(),available=r.GetProperty("available").GetBoolean()};
            }
            else if(kind=="create")
            {
                WriteFile(root,path!,"99AABBCC",17); expected=new {created=true};
            }
            else throw new InvalidDataException("Unknown fixture operation");
            return new {input=spec,ok=true,expected};
        }
        catch(Exception e){return new {input=spec,ok=false,status=BinaryProviderProtocol.Classify(e)};}
    }
    public static async Task<int> Main(string[] args)
    {
        var root=Path.GetFullPath(args[0]);
        Directory.CreateDirectory(root);
        foreach(var spec in Files)
        {
            var f=JsonSerializer.SerializeToElement(spec);
            WriteFile(root,f.GetProperty("path").GetString()!,f.GetProperty("data").GetString()!,f.GetProperty("count").GetInt32());
        }
        File.SetAttributes(Path.Combine(root,"hidden.iff"),FileAttributes.Hidden|FileAttributes.System);
        var steps=new List<object>();
        foreach(var path in new[]{"char/sig/png10000.iff","CHAR\\SIG\\PNG10000.IFF"," char/sig/png10000.iff ","png10000.iff","empty.iff","hidden.iff","../x","/x","C:\\x","char//sig/a.iff","char/./sig/a.iff","","unicode/É.IFF","unicode/SS.iff","unicode/I.iff","unicode/S.iff","unicode/σ.iff","unicode/ς.iff","unicode/𐐨.iff"}) steps.Add(Step("query",path));
        foreach(var path in new[]{"char/sig/png10000.iff","empty.iff","balls/happy.iff"})
            foreach(var offset in new ulong[]{0,1,7})
                foreach(var length in new uint[]{0,1,17,1048576,1048577})
                    if(path!="empty.iff"||offset==0) steps.Add(Step("read",path,offset,length));
        steps.Add(Step("query","alias/copied.iff"));
        steps.Add(Step("alias","char/sig/png10000.iff",source:"balls/happy.iff"));
        steps.Add(Step("alias","mutable/new.iff",source:"BALLS\\HAPPY.IFF"));
        steps.Add(Step("read","mutable/new.iff",0,23));
        steps.Add(Step("create","new/runtime.iff"));
        steps.Add(Step("query","new/runtime.iff"));
        steps.Add(Step("alias","mutable/new.iff",source:"new/runtime.iff"));
        steps.Add(Step("read","mutable/new.iff",0,17));
        steps.Add(Step("alias","mutable/new.iff",source:"empty.iff"));
        steps.Add(Step("alias","mutable/new.iff",source:"../outside.iff"));
        steps.Add(Step("alias","mutable/new.iff",source:"missing.iff"));
        steps.Add(Step("alias","mutable/new.iff"));
        steps.Add(Step("query","mutable/new.iff"));
        steps.Add(Step("alias","mutable/existing.iff",source:"balls/happy.iff"));
        steps.Add(Step("read","mutable/existing.iff",0,23));
        steps.Add(Step("alias","mutable/existing.iff"));
        steps.Add(Step("read","mutable/existing.iff",0,7));
        var aliases=new Dictionary<string,string>{{"alias/copied.iff","char/sig/png10000.iff"},{"mutable/existing.iff","char/sig/png10000.iff"}};
        var mutable=new[]{"mutable/new.iff","mutable/existing.iff"};
        using var service=new ProviderService(root,Token,aliases,mutable);
        var results=new List<object>();
        foreach(var step in steps) results.Add(await Run(service,root,step));
        var comparisons=new[]{("I","ı"),("s","ſ"),("ß","SS"),("Σ","ς"),("𐐀","𐐨"),("é","É")}.Select(p=>new {left=p.Item1,right=p.Item2,equal=StringComparer.OrdinalIgnoreCase.Equals(p.Item1,p.Item2)});
        Console.WriteLine(JsonSerializer.Serialize(new {schema="nba2k27-provider-assets-oracle-v1",files=Files,aliases,mutable,steps=results,comparisons,folds=OrdinalTable(),runtime=Environment.Version.ToString(),gameAccess=false,networkRequests=0}));
        return 0;
    }
}
