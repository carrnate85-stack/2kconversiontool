using System;
using System.Collections.Generic;
using System.Globalization;

namespace NBA2K27RosterResearch
{
    internal sealed class PlayerBioData
    {
        internal int Year, Month, Day, YearsPro;
        internal string DateText()
        {
            try { return new DateTime(Year, Month, Day).ToString("yyyy-MM-dd", CultureInfo.InvariantCulture); }
            catch (ArgumentOutOfRangeException) { return null; }
        }
    }
    internal sealed class BioPatch
    {
        internal int Offset;
        internal byte[] Before, After;
    }
    // Layout comes from the exact RatingSchema-fingerprinted serializer, not
    // 2K23 offsets. Caller must validate live identity/owner before using patches.
    internal static class PlayerBio
    {
        internal static PlayerBioData Read(byte[] record)
        {
            if(record==null || record.Length<0x3E3)throw new InvalidOperationException("Incomplete birthday/years-pro record.");
            return new PlayerBioData { Year=BitConverter.ToUInt16(record,0x112)&0xFFF,
                Month=record[0x36B]>>4,Day=(record[0x114]>>1)&31,YearsPro=(record[0x3E2]>>1)&31 };
        }
        internal static DateTime Date(string value)
        {
            if(value==null || value.Length!=10 || value[4]!='-' || value[7]!='-')throw new InvalidOperationException("Birthday must be YYYY-MM-DD.");
            for(int i=0;i<value.Length;i++)if(i!=4 && i!=7 && (value[i]<'0'||value[i]>'9'))throw new InvalidOperationException("Birthday must be YYYY-MM-DD.");
            DateTime parsed;
            if(!DateTime.TryParseExact(value,"yyyy-MM-dd",CultureInfo.InvariantCulture,DateTimeStyles.None,out parsed) || parsed.Year>4095)
                throw new InvalidOperationException("Birthday must be a real calendar date between years 0001 and 4095.");
            return parsed;
        }
        private static void Add(List<BioPatch> result,byte[] record,int offset,byte[] after)
        {
            byte[] before=new byte[after.Length];Array.Copy(record,offset,before,0,before.Length);
            bool changed=false;for(int i=0;i<before.Length;i++)if(before[i]!=after[i])changed=true;
            if(changed)result.Add(new BioPatch { Offset=offset,Before=before,After=after });
        }
        internal static List<BioPatch> Plan(byte[] record,string birthday,int? expectedYear,int? expectedMonth,int? expectedDay,int? yearsPro,int? expectedYearsPro)
        {
            PlayerBioData current=Read(record);
            var result=new List<BioPatch>();
            if(birthday!=null)
            {
                DateTime desired=Date(birthday);
                if(!expectedYear.HasValue||!expectedMonth.HasValue||!expectedDay.HasValue
                    ||current.Year!=expectedYear.Value||current.Month!=expectedMonth.Value||current.Day!=expectedDay.Value)
                    throw new InvalidOperationException("The original birthday changed before Save. Refresh before saving.");
                ushort year=(ushort)((BitConverter.ToUInt16(record,0x112)&0xF000)|desired.Year);
                Add(result,record,0x112,BitConverter.GetBytes(year));
                Add(result,record,0x114,new byte[]{(byte)((record[0x114]&~0x3E)|(desired.Day<<1))});
                Add(result,record,0x36B,new byte[]{(byte)((record[0x36B]&0xF)|(desired.Month<<4))});
            }
            if(yearsPro.HasValue)
            {
                if(yearsPro.Value<0||yearsPro.Value>31||!expectedYearsPro.HasValue||expectedYearsPro.Value<0||expectedYearsPro.Value>31)
                    throw new InvalidOperationException("Years pro must be 0 through 31 with a verified original value.");
                if(current.YearsPro!=expectedYearsPro.Value)throw new InvalidOperationException("The original years pro changed before Save. Refresh before saving.");
                Add(result,record,0x3E2,new byte[]{(byte)((record[0x3E2]&~0x3E)|(yearsPro.Value<<1))});
            }
            return result;
        }
    }
}
