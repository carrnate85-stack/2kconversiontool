using System;
using System.Collections.Generic;
using System.Reflection;
using System.Web.Script.Serialization;

// Build-time oracle only. Invoke a closed list of pure original C# methods;
// never invoke Main/Run/Build or any process/Win32/read/write method.
internal static class ScalerPolicyFixture {
    static BindingFlags Flags=BindingFlags.Static|BindingFlags.NonPublic;
    static uint B(float f){return BitConverter.ToUInt32(BitConverter.GetBytes(f),0);}
    static float F(uint b){return BitConverter.ToSingle(BitConverter.GetBytes(b),0);}
    public static int Main(string[] args){
        if(args.Length!=1)return 2;
        var assembly=Assembly.LoadFrom(args[0]);
        var policy=assembly.GetType("ScalerPolicy",true);
        var helper=assembly.GetType("PlayerScaler",true);
        var target=policy.GetMethod("Target",Flags);
        var owned=policy.GetMethod("Owned",Flags);
        var pair=helper.GetMethod("ScalePair",Flags);
        var arenaOwned=helper.GetMethod("OwnedArenaScale",Flags);
        var actorType=assembly.GetType("ArenaCharacterOptIn",true);
        var targets=new List<object>();
        var targetBits=new List<uint>();
        for(int p=95;p<=100;p++){
            uint bits=(uint)target.Invoke(null,new object[]{p});
            targets.Add(new{percent=p,bits=bits});targetBits.Add(bits);
        }
        var ownedRows=new List<object>();
        var currents=new List<uint>(targetBits);currents.Add(0);currents.Add(0x7fc00000);currents.Add(0x3f800000);
        foreach(uint a in targetBits)foreach(uint r in targetBits)foreach(uint c in currents){
            bool result=(bool)owned.Invoke(null,new object[]{c,a,r});
            ownedRows.Add(new{current=c,applied=a,requested=r,result=result});
        }
        var pairRows=new List<object>();
        var seeds=new List<object>();
        var arenaRows=new List<object>();
        uint[] originals={B(0.15f),B(0.5f),B(0.8f),B(0.98f),B(1f),B(1.2f),B(1.7f),B(5f),B(9f)};
        foreach(uint original in originals){
            uint originalInverse=B(1f/F(original));
            foreach(uint t in targetBits){
                // Exact unchanged original helper seed expression:
                // B(F(owned.originalScale)*factor), B(1f/F(desiredScale)).
                float factor=F(t);uint scale=B(F(original)*factor),inverse=B(1f/F(scale));
                seeds.Add(new{original=original,target=t,scale=scale,inverse=inverse});
                pairRows.Add(new{scale=scale,inverse=inverse,result=(bool)pair.Invoke(null,new object[]{scale,inverse})});
                var actor=Activator.CreateInstance(actorType);
                actorType.GetField("originalScale").SetValue(actor,original);
                actorType.GetField("originalInverse").SetValue(actor,originalInverse);
                arenaRows.Add(new{original=original,originalInverse=originalInverse,scale=scale,inverse=inverse,result=(bool)arenaOwned.Invoke(null,new object[]{actor,scale,inverse})});
            }
            pairRows.Add(new{scale=original,inverse=originalInverse,result=(bool)pair.Invoke(null,new object[]{original,originalInverse})});
            pairRows.Add(new{scale=original,inverse=B(F(originalInverse)*1.003f),result=(bool)pair.Invoke(null,new object[]{original,B(F(originalInverse)*1.003f)})});
        }
        foreach(uint b in new uint[]{0,B(0.1f),B(10f),0x7fc00000,0x7f800000,0xff800000,0x80000000}){
            pairRows.Add(new{scale=b,inverse=B(1f),result=(bool)pair.Invoke(null,new object[]{b,B(1f)})});
            pairRows.Add(new{scale=B(1f),inverse=b,result=(bool)pair.Invoke(null,new object[]{B(1f),b})});
        }
        Console.WriteLine(new JavaScriptSerializer().Serialize(new{schema="nba2k27-scaler-policy-oracle-v1",gameAccess=false,targets=targets,owned=ownedRows,pairs=pairRows,seeds=seeds,arenaOwned=arenaRows}));
        return 0;
    }
}
