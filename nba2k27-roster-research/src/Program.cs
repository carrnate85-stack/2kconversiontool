using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;

namespace NBA2K27RosterResearch
{
    internal static class Native
    {
        internal const uint QueryInformation = 0x0400;
        internal const uint VmRead = 0x0010;
        internal const uint VmWrite = 0x0020;
        internal const uint VmOperation = 0x0008;
        internal const uint MemCommit = 0x1000;
        internal const uint PageNoAccess = 0x01;
        internal const uint PageGuard = 0x100;

        [StructLayout(LayoutKind.Sequential)]
        internal struct MemoryBasicInformation
        {
            internal IntPtr BaseAddress;
            internal IntPtr AllocationBase;
            internal uint AllocationProtect;
            internal ushort PartitionId;
            internal UIntPtr RegionSize;
            internal uint State;
            internal uint Protect;
            internal uint Type;
        }

        [DllImport("kernel32.dll", SetLastError = true)]
        internal static extern IntPtr OpenProcess(uint access, bool inheritHandle, int processId);

        [DllImport("kernel32.dll")]
        internal static extern bool CloseHandle(IntPtr handle);

        [DllImport("kernel32.dll")]
        internal static extern UIntPtr VirtualQueryEx(
            IntPtr process,
            IntPtr address,
            out MemoryBasicInformation information,
            UIntPtr length);

        [DllImport("kernel32.dll", SetLastError = true)]
        internal static extern bool ReadProcessMemory(
            IntPtr process,
            IntPtr address,
            [Out] byte[] buffer,
            UIntPtr size,
            out UIntPtr bytesRead);

        [DllImport("kernel32.dll", SetLastError = true)]
        internal static extern bool WriteProcessMemory(
            IntPtr process,
            IntPtr address,
            byte[] buffer,
            UIntPtr size,
            out UIntPtr bytesWritten);
    }

    internal sealed class Reader : IDisposable
    {
        internal readonly int ProcessId;
        private IntPtr handle;

        internal Reader(int processId, bool allowWrites = false)
        {
            ProcessId = processId;
            uint access = Native.QueryInformation | Native.VmRead;
            if (allowWrites) access |= Native.VmWrite | Native.VmOperation;
            handle = Native.OpenProcess(access, false, processId);
            if (handle == IntPtr.Zero)
                throw new InvalidOperationException("Read-only process access failed: " + Marshal.GetLastWin32Error());
        }

        internal void Write(long address, byte[] value)
        {
            UIntPtr written;
            if (!Native.WriteProcessMemory(handle, new IntPtr(address), value, (UIntPtr)value.Length, out written)
                || written.ToUInt64() != (ulong)value.Length)
                throw new InvalidOperationException("WriteProcessMemory failed at 0x" + address.ToString("X"));
        }

        internal byte[] Read(long address, int size)
        {
            byte[] buffer = new byte[size];
            UIntPtr received;
            if (!Native.ReadProcessMemory(handle, new IntPtr(address), buffer, (UIntPtr)size, out received)
                || received.ToUInt64() != (ulong)size)
                throw new InvalidOperationException("ReadProcessMemory failed at 0x" + address.ToString("X"));
            return buffer;
        }

        internal IEnumerable<Native.MemoryBasicInformation> Regions()
        {
            long cursor = 0;
            const long maximum = 0x00007FFFFFFFFFFF;
            while (cursor < maximum)
            {
                Native.MemoryBasicInformation information;
                UIntPtr result = Native.VirtualQueryEx(
                    handle,
                    new IntPtr(cursor),
                    out information,
                    (UIntPtr)Marshal.SizeOf(typeof(Native.MemoryBasicInformation)));
                if (result == UIntPtr.Zero) yield break;
                long baseAddress = information.BaseAddress.ToInt64();
                ulong rawSize = information.RegionSize.ToUInt64();
                if (rawSize > Int64.MaxValue) yield break;
                long size = (long)rawSize;
                yield return information;
                long step = Math.Max(size, 0x1000);
                if (baseAddress > maximum - step) yield break;
                long next = baseAddress + step;
                if (next <= cursor) yield break;
                cursor = next;
            }
        }

        public void Dispose()
        {
            if (handle != IntPtr.Zero)
            {
                Native.CloseHandle(handle);
                handle = IntPtr.Zero;
            }
        }
    }

    internal sealed class Team
    {
        internal int Index;
        internal long Address;
        internal string Name;
        internal string CityName;
        internal string CityShort;
        internal string StateShort;
        internal string MainLogo;
        internal ushort UniqueId;
        internal int RosterType;
        internal string RosterTypeName;
        internal int HistoricYearId;
        internal string HistoricYear;
        internal ushort? ArenaId;
        internal string ArenaName;
        internal string ArenaFile;
        internal List<TeamUniform> Uniforms = new List<TeamUniform>();
        internal List<int> EmptyUniformSlots = new List<int>();
        internal List<long> UniformPointerAddresses = new List<long>();
        internal int InvalidUniformSlotCount;
    }

    internal sealed class TeamUniform
    {
        internal int Slot;
        internal int Index;
        internal long Address;
        internal ushort Id;
        internal ushort TeamId;
        internal string BaseFilename;
        internal string SponsorLogoName;
        internal ulong LocalizedNameToken;
        internal ulong LocalizedTeamNameToken;
        internal string Name;
        internal string TeamName;
        internal uint UniqueSignatureId;
        internal uint SockClothingItemName;
        internal uint SockClothingItemNameAway;
        internal uint MyTeamTheme;
        internal uint BenchPlayerShirt;
        internal uint SockOption1;
        internal uint SockOption2;
        internal uint SockOption3;
        internal uint SockOption4;
        internal uint SockOption5;
        internal int Type;
        internal int OriginalType;
        internal int LogoType;
        internal int LogoBrand;
        internal int DominantColor;
        internal bool ShouldScorebugUseUniformColor;
        internal bool UseTeamLogoOnUniform;
        internal int CustomCasualShirtColor;
        internal int CustomJerseyColorRed;
        internal int CustomJerseyColorGreen;
        internal int CustomJerseyColorBlue;
        internal int CustomNameColorRed;
        internal int CustomNameColorGreen;
        internal int CustomNameColorBlue;
        internal int CustomShoeTrimColor1;
        internal int CustomShoeTrimColor2;
        internal int CustomShoeDominantColor;
        internal bool DisablePhoto;
        internal int Side;
        internal int Designation;
        internal bool IsAlternate;
        internal int HomePriority;
        internal int AwayPriority;
        internal int TopNamePlateStyle;
        internal int BottomNamePlateStyle;
        internal bool OverrideNbaPatch;
        internal bool HasNumberOnShorts;
        internal bool PrimaryLogoSockColor;
        internal bool SecondaryLogoSockColor;
        internal bool TertiaryLogoSockColor;
        internal bool QuaternaryLogoSockColor;
        internal bool QuintaryLogoSockColor;
        internal bool RightAlignFrontNumber;
        internal uint UniformPrimaryColor;
        internal uint UniformSecondaryColor;
        internal uint PrimaryColor;
        internal uint SecondaryColor;
        internal uint TertiaryColor;
        internal uint QuaternaryColor;
        internal uint QuintaryColor;
        internal uint SextaryColor;
        internal uint PrimaryAccessoryColor;
        internal uint SecondaryAccessoryColor;
        internal uint TertiaryAccessoryColor;
        internal uint QuaternaryAccessoryColor;
        internal uint QuintaryAccessoryColor;
        internal uint UnderShirtColorHome;
        internal uint UnderShirtColorAway;
        internal uint ShoePrimaryColorAway;
        internal uint PrimarySockColor;
        internal uint SecondarySockColor;
        internal uint TertiarySockColor;
        internal uint QuaternarySockColor;
        internal uint QuintarySockColor;
        internal int HeadbandColorHome;
        internal int HeadbandColorAway;
        internal int HeadbandLogoType;
        internal int LegAccessoryColorHome;
        internal int LegAccessoryColorAway;
        internal int ArmItemColorHome;
        internal int ArmItemColorAway;
        internal int SockColorHome;
        internal int SockColorAway;
        internal uint Flags160;
        internal uint Flags164;
        internal uint Flags168;
        internal uint Flags16C;
        internal bool HasCustomUniformData;
        internal long CustomUniformAddress;
        internal ushort CustomUniformUniqueId;
        internal uint CustomUniformSignature;
        internal string CustomUniformName;
        internal uint CustomUniformFontNameToken;
        internal int CustomUniformNumberOfLayers;
        internal int CustomUniformNumberOfOptions;
        internal bool CustomUniformInUse;
        internal uint CustomUniformPadding;
        internal string BakedJerseyFilename;
        internal string BakedShortsFilename;

        internal bool StructurallyBlank()
        {
            return Id == 0 && TeamId == 0
                && String.IsNullOrWhiteSpace(BaseFilename)
                && String.IsNullOrWhiteSpace(SponsorLogoName)
                && String.IsNullOrWhiteSpace(Name)
                && String.IsNullOrWhiteSpace(TeamName)
                && LocalizedNameToken == 0 && LocalizedTeamNameToken == 0
                && UniqueSignatureId == 0 && SockClothingItemName == 0
                && SockClothingItemNameAway == 0 && MyTeamTheme == 0
                && BenchPlayerShirt == 0 && SockOption1 == 0 && SockOption2 == 0
                && SockOption3 == 0 && SockOption4 == 0 && SockOption5 == 0
                && Flags160 == 0 && Flags164 == 0 && Flags168 == 0 && Flags16C == 0
                && UniformPrimaryColor == 0 && UniformSecondaryColor == 0
                && PrimaryColor == 0 && SecondaryColor == 0 && TertiaryColor == 0
                && QuaternaryColor == 0 && QuintaryColor == 0 && SextaryColor == 0
                && PrimaryAccessoryColor == 0 && SecondaryAccessoryColor == 0
                && TertiaryAccessoryColor == 0 && QuaternaryAccessoryColor == 0
                && QuintaryAccessoryColor == 0 && UnderShirtColorHome == 0
                && UnderShirtColorAway == 0 && ShoePrimaryColorAway == 0
                && PrimarySockColor == 0 && SecondarySockColor == 0
                && TertiarySockColor == 0 && QuaternarySockColor == 0
                && QuintarySockColor == 0 && !HasCustomUniformData;
        }

        internal TeamUniform ForSlot(int slot)
        {
            TeamUniform copy = (TeamUniform)MemberwiseClone();
            copy.Slot = slot;
            return copy;
        }
    }

    internal sealed class Arena
    {
        internal int Index;
        internal long Address;
        internal string Name;
        internal string ArenaFile;
        internal string FloorFile;
        internal ushort Id;
        internal bool EmptyCandidate;
    }

    internal sealed class Player
    {
        internal int Index;
        internal long Address;
        internal string FirstName;
        internal string LastName;
        internal long TeamAddress;
        internal int? TeamIndex;
        internal string TeamName;
        internal ushort CyberfaceId;
        internal int PositionId;
        internal int SecondaryPositionId;
        internal string Position;
        internal string SecondaryPosition;
        internal int JerseyNumberRaw;
        internal string JerseyNumber;
        internal float? WeightLbs;
        internal ushort? HeightRaw;
        internal double? HeightCm;
        internal ushort? WingspanRaw;
        internal double? WingspanCm;
        internal int PotentialRaw;
        internal int Potential;
        internal string AudioJson = "null";
        internal string RatingsJson = "null";
        internal int? YearsPro;
        internal bool BioLayoutVerified;
        internal int? BirthYear, BirthMonth, BirthDay;
        internal string BirthDate;
        internal int? FromIndex;
        internal string CollegeName;
        internal string ArchetypeKey;
        internal float? CachedOverallRaw;
        internal int? Overall;
        internal int ShoeHome;
        internal int ShoeAway;
        internal int SockHome;
        internal int SockAway;
        internal int Undershirt;
        internal int CompressionShorts;
        internal int Headband;
        internal int LeftArm;
        internal int RightArm;
        internal int LeftElbow;
        internal int RightElbow;
        internal int LeftWrist;
        internal int RightWrist;
        internal int LeftFingers;
        internal int RightFingers;
        internal int LeftLeg;
        internal int RightLeg;
        internal int LeftKnee;
        internal int RightKnee;
        internal int LeftAnkle;
        internal int RightAnkle;
    }

    internal sealed class WriteOperation
    {
        internal int Offset;
        internal byte[] Before;
        internal byte[] After;
    }

    internal sealed class AbsoluteWriteOperation
    {
        internal long Address;
        internal byte[] Before;
        internal byte[] After;
    }

    internal static partial class Program
    {
        private const long MinRegionSize = 0x800000;
        private const long MaxRegionSize = 0x8000000;
        private const int PlayerStride = 0x4F8;
        private const int PlayerNameBytes = 0x50;
        private const int NamePartBytes = 0x28;
        private const int PlayerCountOffset = 0x60;
        private const int PlayerPointerOffset = 0x68;
        private const int PlayerTableDelta = 0x680;
        private const int PlayerCyberfaceIdOffset = 0x110;
        private const int PlayerAppearancePointerOffset = 0x78;
        private const int PlayerWeightOffset = 0x108;
        private const int PlayerJerseyOffset = 0x115;
        private const int PlayerPositionOffset = 0x12D;
        private const int PlayerCachedOverallOffset = 0x1AC;
        private const int PlayerPotentialOffset = 0x45D;
        private const int TeamCountOffset = 0xD0;
        private const int TeamPointerOffset = 0xD8;
        private const int TeamStride = 0x1550;
        private const int TeamNameOffset = 0x2FA;
        private const int TeamCityOffset = 0x32C;
        private const int TeamCityShortOffset = 0x350;
        private const int TeamStateShortOffset = 0x35E;
        private const int TeamMainLogoOffset = 0x1AE;
        private const int TeamArenaPointerOffset = 0xEE0;
        private const int TeamUniqueIdOffset = 0xF20;
        private const int TeamRosterTypeOffset = 0x123F;
        private const int TeamHistoricYearOffset = 0x154A;
        private const int TeamUniformSlotOffset = 0xDE0;
        private const int TeamUniformSlotCount = 32;
        private const int UniformCountOffset = 0x290;
        private const int UniformPointerOffset = 0x298;
        private const int UniformStride = 0x170;
        private const int ArenaCountOffset = 0xB0;
        private const int ArenaPointerOffset = 0xB8;
        private const int ArenaStride = 0x1278;
        private const int ArenaNameOffset = 0x0;
        private const int ArenaFileOffset = 0x7E;
        private const int ArenaFloorOffset = 0xA4;
        private const int ArenaIdOffset = 0x792;

        private static ulong U64(byte[] value, int offset)
        {
            return BitConverter.ToUInt64(value, offset);
        }

        private static int Bits(byte[] value, int offset, int bitStart, int bitLength)
        {
            int byteCount = (bitStart + bitLength + 7) / 8;
            ulong packed = 0;
            for (int index = 0; index < byteCount; index++) packed |= (ulong)value[offset + index] << (index * 8);
            return (int)((packed >> bitStart) & ((1UL << bitLength) - 1));
        }

        private static string PositionName(int value)
        {
            string[] positions = { "PG", "SG", "SF", "PF", "C", "N/A" };
            return value >= 0 && value < positions.Length ? positions[value] : "Unknown";
        }

        private static int PotentialRating(int raw)
        {
            return Math.Min(110, 25 + (raw + 1) / 3);
        }

        private static string DecodeUtf16(byte[] value, int offset, int length)
        {
            int end = offset + length;
            for (int index = offset; index + 1 < offset + length; index += 2)
            {
                if (value[index] == 0 && value[index + 1] == 0)
                {
                    end = index;
                    break;
                }
            }
            return Encoding.Unicode.GetString(value, offset, Math.Max(0, end - offset)).Trim();
        }

        private static string DecodeAscii(byte[] value, int offset, int length)
        {
            int end = offset;
            int limit = Math.Min(value.Length, offset + length);
            while (end < limit && value[end] != 0) end++;
            return Encoding.ASCII.GetString(value, offset, Math.Max(0, end - offset)).Trim();
        }

        private static string RosterTypeName(int value)
        {
            string[] values = { "Normal", "Historic", "User", "Online", "All Stars East", "All Stars West", "Street Home", "Street Away", "Rookies", "Sophomores", "Rookie Showcase", "MyTeam", "Euroleague", "Domination", "College", "2K Sports", "Hoops Summit", "G-League", "College", "Unknown", "Team USA", "Dream Team", "National Team", "Expansion Slot", "Expansion Premade", "E-League", "All-Time", "Unknown", "WNBA", "Decade" };
            return value >= 0 && value < values.Length ? values[value] : "Unknown";
        }

        private static string HistoricYearName(int value)
        {
            if (value <= 0) return "";
            int first = (value - 1) % 100;
            int second = (first + 1) % 100;
            return "'" + first.ToString("00", CultureInfo.InvariantCulture) + "-'" + second.ToString("00", CultureInfo.InvariantCulture);
        }

        private static bool ValidName(string value)
        {
            if (String.IsNullOrWhiteSpace(value) || value.Length > 40 || value.IndexOf('\uFFFD') >= 0) return false;
            for (int index = 0; index < value.Length; index++)
                if (Char.IsControl(value[index])) return false;
            return true;
        }

        private static string Json(string value)
        {
            if (value == null) return "null";
            StringBuilder result = new StringBuilder(value.Length + 8).Append('"');
            foreach (char character in value)
            {
                switch (character)
                {
                    case '"': result.Append("\\\""); break;
                    case '\\': result.Append("\\\\"); break;
                    case '\b': result.Append("\\b"); break;
                    case '\f': result.Append("\\f"); break;
                    case '\n': result.Append("\\n"); break;
                    case '\r': result.Append("\\r"); break;
                    case '\t': result.Append("\\t"); break;
                    default:
                        if (character < 0x20) result.Append("\\u").Append(((int)character).ToString("X4"));
                        else result.Append(character);
                        break;
                }
            }
            return result.Append('"').ToString();
        }

        private static string Argument(string[] args, string name)
        {
            for (int index = 0; index + 1 < args.Length; index++)
                if (args[index] == name) return args[index + 1];
            throw new InvalidOperationException("Missing required argument " + name + ".");
        }

        private static byte[] FixedUtf16(string value, int byteLength, string fieldName)
        {
            if (value == null) value = "";
            byte[] encoded = Encoding.Unicode.GetBytes(value);
            if (encoded.Length > byteLength - 2)
                throw new InvalidOperationException(fieldName + " exceeds its " + ((byteLength / 2) - 1) + " character limit.");
            byte[] result = new byte[byteLength];
            Buffer.BlockCopy(encoded, 0, result, 0, encoded.Length);
            return result;
        }

        private static byte[] FixedAscii(string value, int byteLength, string fieldName)
        {
            if (value == null) value = "";
            for (int index = 0; index < value.Length; index++)
                if (value[index] < 0x20 || value[index] > 0x7E)
                    throw new InvalidOperationException(fieldName + " must contain printable ASCII characters only.");
            byte[] encoded = Encoding.ASCII.GetBytes(value);
            if (encoded.Length > byteLength - 1)
                throw new InvalidOperationException(fieldName + " exceeds its " + (byteLength - 1) + " character limit.");
            byte[] result = new byte[byteLength];
            Buffer.BlockCopy(encoded, 0, result, 0, encoded.Length);
            return result;
        }

        private static bool SameBytes(byte[] left, byte[] right)
        {
            if (left.Length != right.Length) return false;
            for (int index = 0; index < left.Length; index++) if (left[index] != right[index]) return false;
            return true;
        }

        private static byte[] Slice(byte[] value, int offset, int length)
        {
            byte[] result = new byte[length];
            Buffer.BlockCopy(value, offset, result, 0, length);
            return result;
        }

        private static WriteOperation Operation(byte[] record, int offset, byte[] after)
        {
            return new WriteOperation { Offset = offset, Before = Slice(record, offset, after.Length), After = after };
        }

        private static AbsoluteWriteOperation AbsoluteOperation(Reader memory, long address, byte[] after)
        {
            return new AbsoluteWriteOperation { Address = address, Before = memory.Read(address, after.Length), After = after };
        }

        // Prepare one-byte patches, preserving every neighbouring packed field.
        private static byte[] PatchRatingFields(byte[] record, string changes)
        {
            byte[] result=(byte[])record.Clone(); var seen=new HashSet<int>();
            foreach(string change in changes.Split(';'))
            {
                string[] parts=change.Split(':'); int index,expected,value;
                if(parts.Length!=3 || !Int32.TryParse(parts[0],out index) || !Int32.TryParse(parts[1],out expected)
                    || !Int32.TryParse(parts[2],out value) || index<0 || index>=RatingSchema.Fields.GetLength(0) || !seen.Add(index))
                    throw new InvalidOperationException("Invalid or duplicate player field change.");
                int offset=RatingSchema.Fields[index,0],shift=RatingSchema.Fields[index,1],bits=RatingSchema.Fields[index,2];
                uint mask=bits==32?UInt32.MaxValue:(1u<<bits)-1,raw=BitConverter.ToUInt32(record,offset);
                if(value<0 || value>RatingSchema.EditMax[index] || expected<0 || (uint)expected>mask || ((raw>>shift)&mask)!=(uint)expected)
                    throw new InvalidOperationException("Player field changed or requested value is invalid. Refresh before saving.");
                uint packed=BitConverter.ToUInt32(result,offset);
                packed=(packed&~(mask<<shift))|((uint)value<<shift);
                Buffer.BlockCopy(BitConverter.GetBytes(packed),0,result,offset,4);
            }
            return result;
        }

        private static int RunPlayerEdit(string[] args)
        {
            int pid = ProcessIdFromArgs(args);
            int recordIndex = Int32.Parse(Argument(args, "--record-index"), CultureInfo.InvariantCulture);
            string expectedFirst = Argument(args, "--expected-first");
            string expectedLast = Argument(args, "--expected-last");
            ushort expectedCyberface = UInt16.Parse(Argument(args, "--expected-cyberface"), CultureInfo.InvariantCulture);
            int expectedPosition = Int32.Parse(Argument(args, "--expected-position"), CultureInfo.InvariantCulture);
            int expectedSecondary = Int32.Parse(Argument(args, "--expected-secondary"), CultureInfo.InvariantCulture);
            int expectedJersey = Int32.Parse(Argument(args, "--expected-jersey"), CultureInfo.InvariantCulture);
            int expectedPotentialRaw = Int32.Parse(Argument(args, "--expected-potential-raw"), CultureInfo.InvariantCulture);
            ushort expectedHeightRaw = UInt16.Parse(Argument(args, "--expected-height-raw"), CultureInfo.InvariantCulture);
            ushort expectedWingspanRaw = UInt16.Parse(Argument(args, "--expected-wingspan-raw"), CultureInfo.InvariantCulture);
            string first = Argument(args, "--first");
            string last = Argument(args, "--last");
            ushort cyberface = UInt16.Parse(Argument(args, "--cyberface"), CultureInfo.InvariantCulture);
            int position = Int32.Parse(Argument(args, "--position"), CultureInfo.InvariantCulture);
            int secondary = Int32.Parse(Argument(args, "--secondary"), CultureInfo.InvariantCulture);
            int jersey = Int32.Parse(Argument(args, "--jersey"), CultureInfo.InvariantCulture);
            float weight = Single.Parse(Argument(args, "--weight"), CultureInfo.InvariantCulture);
            ushort heightRaw = UInt16.Parse(Argument(args, "--height-raw"), CultureInfo.InvariantCulture);
            ushort wingspanRaw = UInt16.Parse(Argument(args, "--wingspan-raw"), CultureInfo.InvariantCulture);
            int potentialRaw = Int32.Parse(Argument(args, "--potential-raw"), CultureInfo.InvariantCulture);
            bool editAudio = Array.IndexOf(args, "--full-name-id") >= 0;
            bool editRatings = Array.IndexOf(args, "--rating-edits") >= 0;
            bool editFrom = Array.IndexOf(args, "--from-index") >= 0;
            bool editBirth = Array.IndexOf(args, "--birth-date") >= 0;
            bool editYearsPro = Array.IndexOf(args, "--years-pro") >= 0;
            bool editBio = editBirth || editYearsPro;
            string birthday = editBirth ? Argument(args,"--birth-date") : null;
            int? expectedBirthYear = editBirth ? (int?)Int32.Parse(Argument(args,"--expected-birth-year"),CultureInfo.InvariantCulture) : null;
            int? expectedBirthMonth = editBirth ? (int?)Int32.Parse(Argument(args,"--expected-birth-month"),CultureInfo.InvariantCulture) : null;
            int? expectedBirthDay = editBirth ? (int?)Int32.Parse(Argument(args,"--expected-birth-day"),CultureInfo.InvariantCulture) : null;
            int? yearsPro = editYearsPro ? (int?)Int32.Parse(Argument(args,"--years-pro"),CultureInfo.InvariantCulture) : null;
            int? expectedYearsPro = editYearsPro ? (int?)Int32.Parse(Argument(args,"--expected-years-pro"),CultureInfo.InvariantCulture) : null;
            int fromIndex = editFrom ? Int32.Parse(Argument(args,"--from-index"),CultureInfo.InvariantCulture) : -1;
            int expectedFromIndex = editFrom ? Int32.Parse(Argument(args,"--expected-from-index"),CultureInfo.InvariantCulture) : -1;
            string fromName = editFrom ? Argument(args,"--from-name") : null;
            int fullNameId = editAudio ? Int32.Parse(Argument(args, "--full-name-id"), CultureInfo.InvariantCulture) : -1;
            int expectedFullNameId = editAudio ? Int32.Parse(Argument(args, "--expected-full-name-id"), CultureInfo.InvariantCulture) : -1;
            if (editAudio && (fullNameId < -1 || fullNameId > Int16.MaxValue || expectedFullNameId < -1 || expectedFullNameId > Int16.MaxValue))
                throw new InvalidOperationException("Full-name sound ID must be -1 or 0 through 32767.");
            if ((!ValidName(first) && !ValidName(last)) || first.Length > 19 || last.Length > 19)
                throw new InvalidOperationException("A player needs a valid first or last name, limited to 19 characters each.");
            if (position < 0 || position > 5 || secondary < 0 || secondary > 5)
                throw new InvalidOperationException("Player positions must use IDs 0 through 5.");
            if (jersey < 0 || jersey > 100) throw new InvalidOperationException("Jersey must be 0–99 or 100 for 00.");
            if (Single.IsNaN(weight) || Single.IsInfinity(weight) || weight < 70 || weight > 500)
                throw new InvalidOperationException("Weight must be from 70 to 500 pounds.");
            if (heightRaw < 12000 || heightRaw > 26000 || wingspanRaw < 12000 || wingspanRaw > 32000)
                throw new InvalidOperationException("Height or wingspan is outside the verified range.");
            if (potentialRaw < 0 || potentialRaw > 255) throw new InvalidOperationException("Potential encoding must fit in one byte.");

            using (Reader memory = new Reader(pid, true))
            {
                long regionBase, regionSize, playerTable;
                int playerCount;
                if (!LocateDatabase(memory, out regionBase, out regionSize, out playerCount, out playerTable))
                    throw new InvalidOperationException("A unique validated NBA 2K27 database was not found.");
                if (recordIndex < 0 || recordIndex >= playerCount)
                    throw new InvalidOperationException("The requested player record is outside the live directory.");
                long playerAddress = playerTable + (long)recordIndex * PlayerStride;
                byte[] record = memory.Read(playerAddress, PlayerStride);
                OriginDirectory originDirectory=null;
                long fromPointer=0;
                if(editFrom) {
                    originDirectory=ReadOriginDirectory(memory,playerTable);
                    if(fromIndex<0||fromIndex>=originDirectory.Count||!originDirectory.Selectable[fromIndex]||originDirectory.Names[fromIndex]!=fromName
                       ||originDirectory.Index(BitConverter.ToInt64(record,0xB0))!=expectedFromIndex||expectedFromIndex < -1)
                        throw new InvalidOperationException("From selection or original player origin changed. Refresh and try again.");
                    fromPointer=originDirectory.Table+(long)fromIndex*0x48;
                }
                byte[] ratingAfter = null;
                List<BioPatch> bioPatches = new List<BioPatch>();
                long ratingOwnerSlot=0;
                if(editRatings || editBio)
                {
                    var ratingPlayer=new Player { Address=playerAddress,FirstName=expectedFirst,LastName=expectedLast };
                    ReadPlayerRatings(memory,playerTable,new List<Player>{ratingPlayer});
                    if(ratingPlayer.RatingsJson=="null" || (editBio&&!ratingPlayer.BioLayoutVerified) || !SameBytes(record,memory.Read(playerAddress,PlayerStride)))
                        throw new InvalidOperationException("Player field layout or ownership could not be verified.");
                    if(editRatings)ratingAfter=PatchRatingFields(record,Argument(args,"--rating-edits"));
                    if(editBio)bioPatches=PlayerBio.Plan(record,birthday,expectedBirthYear,expectedBirthMonth,expectedBirthDay,yearsPro,expectedYearsPro);
                    long image=Process.GetProcessById(pid).MainModule.BaseAddress.ToInt64();
                    long root=BitConverter.ToInt64(memory.Read(image+0x9A47EB8,8),0);
                    ratingOwnerSlot=BitConverter.ToInt64(memory.Read(root+0xA8,8),0)+0x18;
                }
                if (editAudio)
                {
                    var audioPlayer = new Player { Address = playerAddress, FirstName = expectedFirst, LastName = expectedLast };
                    ReadPlayerAudio(memory, playerTable, new List<Player> { audioPlayer });
                    if (audioPlayer.AudioJson == "null" || BitConverter.ToInt16(record, 0x2E8) != expectedFullNameId)
                        throw new InvalidOperationException("The audio mapping is unavailable or changed. Refresh the roster before saving.");
                }
                long appearanceAddress = checked((long)U64(record, PlayerAppearancePointerOffset));
                if (appearanceAddress == 0) throw new InvalidOperationException("The player's appearance record is unavailable.");
                byte[] appearance = memory.Read(appearanceAddress, 4);
                if (DecodeUtf16(record, NamePartBytes, NamePartBytes) != expectedFirst
                    || DecodeUtf16(record, 0, NamePartBytes) != expectedLast
                    || BitConverter.ToUInt16(record, PlayerCyberfaceIdOffset) != expectedCyberface
                    || Bits(record, PlayerPositionOffset, 0, 3) != expectedPosition
                    || Bits(record, PlayerPositionOffset, 3, 3) != expectedSecondary
                    || Bits(record, PlayerJerseyOffset, 3, 7) != expectedJersey
                    || Bits(record, PlayerPotentialOffset, 0, 8) != expectedPotentialRaw
                    || BitConverter.ToUInt16(appearance, 0) != expectedHeightRaw
                    || BitConverter.ToUInt16(appearance, 2) != expectedWingspanRaw)
                    throw new InvalidOperationException("The selected player changed before Save; refresh the roster and try again.");

                ushort jerseyPacked = BitConverter.ToUInt16(record, PlayerJerseyOffset);
                jerseyPacked = (ushort)((jerseyPacked & ~0x3F8) | (jersey << 3));
                byte positionsPacked = (byte)((record[PlayerPositionOffset] & ~0x3F) | position | (secondary << 3));
                byte[] appearanceAfter = new byte[4];
                Buffer.BlockCopy(BitConverter.GetBytes(heightRaw), 0, appearanceAfter, 0, 2);
                Buffer.BlockCopy(BitConverter.GetBytes(wingspanRaw), 0, appearanceAfter, 2, 2);
                List<AbsoluteWriteOperation> operations = new List<AbsoluteWriteOperation> {
                    AbsoluteOperation(memory, playerAddress, FixedUtf16(last, NamePartBytes, "Last name")),
                    AbsoluteOperation(memory, playerAddress + NamePartBytes, FixedUtf16(first, NamePartBytes, "First name")),
                    AbsoluteOperation(memory, playerAddress + PlayerWeightOffset, BitConverter.GetBytes(weight)),
                    AbsoluteOperation(memory, playerAddress + PlayerCyberfaceIdOffset, BitConverter.GetBytes(cyberface)),
                    AbsoluteOperation(memory, playerAddress + PlayerJerseyOffset, BitConverter.GetBytes(jerseyPacked)),
                    AbsoluteOperation(memory, playerAddress + PlayerPositionOffset, new byte[] { positionsPacked }),
                    AbsoluteOperation(memory, playerAddress + PlayerPotentialOffset, new byte[] { (byte)potentialRaw }),
                    AbsoluteOperation(memory, appearanceAddress, appearanceAfter)
                };
                List<AbsoluteWriteOperation> applied = new List<AbsoluteWriteOperation>();
                if(editFrom)operations.Add(new AbsoluteWriteOperation {Address=playerAddress+0xB0,Before=BitConverter.GetBytes(BitConverter.ToInt64(record,0xB0)),After=BitConverter.GetBytes(fromPointer)});
                foreach(BioPatch patch in bioPatches)operations.Add(new AbsoluteWriteOperation {Address=playerAddress+patch.Offset,Before=patch.Before,After=patch.After});
                int ratingWrites=0;
                if(ratingAfter!=null)for(int b=0;b<PlayerStride;b++)if(record[b]!=ratingAfter[b])
                {
                    foreach(var existing in operations)if(playerAddress+b>=existing.Address && playerAddress+b<existing.Address+existing.After.Length)
                        throw new InvalidOperationException("Overlapping player field edits were rejected.");
                    operations.Add(new AbsoluteWriteOperation { Address=playerAddress+b,Before=new byte[]{record[b]},After=new byte[]{ratingAfter[b]} });ratingWrites++;
                }
                if (editAudio)
                {
                    AbsoluteWriteOperation audioOperation = AbsoluteOperation(memory, playerAddress + 0x2E8, BitConverter.GetBytes((short)fullNameId));
                    if (BitConverter.ToInt16(audioOperation.Before, 0) != expectedFullNameId)
                        throw new InvalidOperationException("The full-name audio ID changed before Save; refresh and try again.");
                    operations.Add(audioOperation);
                }
                try
                {
                    foreach (AbsoluteWriteOperation operation in operations)
                    {
                        if(editFrom && (!originDirectory.Stable(memory)||!SameBytes(originDirectory.Bytes,memory.Read(originDirectory.Table,originDirectory.Bytes.Length))))
                            throw new InvalidOperationException("Roster origins changed during Save.");
                        if((editRatings || editBio) && BitConverter.ToInt64(memory.Read(ratingOwnerSlot,8),0)!=playerTable)
                            throw new InvalidOperationException("The roster changed during Save.");
                        if(!SameBytes(memory.Read(operation.Address,operation.Before.Length),operation.Before))
                            throw new InvalidOperationException("A player field changed before its write.");
                        memory.Write(operation.Address, operation.After);
                        applied.Add(operation);
                        if (!SameBytes(memory.Read(operation.Address, operation.After.Length), operation.After))
                            throw new InvalidOperationException("A player-field read-back check failed.");
                    }
                    byte[] identity = memory.Read(playerAddress, PlayerStride);
                    if(editBio)
                    {
                        PlayerBioData verifiedBio=PlayerBio.Read(identity);
                        if(BitConverter.ToInt64(memory.Read(ratingOwnerSlot,8),0)!=playerTable
                            ||(editBirth&&verifiedBio.DateText()!=birthday)||(editYearsPro&&verifiedBio.YearsPro!=yearsPro.Value))
                            throw new InvalidOperationException("Birthday/years-pro final verification failed.");
                    }
                    if (DecodeUtf16(identity, NamePartBytes, NamePartBytes) != first
                        || DecodeUtf16(identity, 0, NamePartBytes) != last
                        || BitConverter.ToUInt16(identity, PlayerCyberfaceIdOffset) != cyberface
                        || (editAudio && BitConverter.ToInt16(identity, 0x2E8) != fullNameId))
                        throw new InvalidOperationException("The player identity failed final verification.");
                }
                catch (Exception error)
                {
                    bool rollbackPassed = true;
                    for (int index = applied.Count - 1; index >= 0; index--)
                    {
                        try
                        {
                            if(editFrom && (!originDirectory.Stable(memory)||!SameBytes(memory.Read(applied[index].Address,applied[index].After.Length),applied[index].After)))
                            { rollbackPassed=false; continue; }
                            if((editRatings || editBio) && (BitConverter.ToInt64(memory.Read(ratingOwnerSlot,8),0)!=playerTable || !SameBytes(memory.Read(applied[index].Address,applied[index].After.Length),applied[index].After)))
                            { rollbackPassed=false; continue; }
                            memory.Write(applied[index].Address, applied[index].Before);
                            rollbackPassed &= SameBytes(memory.Read(applied[index].Address, applied[index].Before.Length), applied[index].Before);
                        }
                        catch (Exception) { rollbackPassed = false; }
                    }
                    throw new InvalidOperationException(error.Message + (rollbackPassed ? " All applied fields were rolled back." : " Rollback verification failed."));
                }
                StringBuilder output = new StringBuilder();
                output.Append("{\"schema\":\"nba2k27-player-edit-v1\",\"ok\":true");
                output.Append(",\"pid\":").Append(pid).Append(",\"gameCodeHooks\":0,\"writes\":").Append(operations.Count);
                output.Append(",\"recordIndex\":").Append(recordIndex).Append(",\"cyberfaceId\":").Append(cyberface);
                output.Append(",\"ratingWrites\":").Append(ratingWrites);
                output.Append(",\"bioWrites\":").Append(bioPatches.Count);
                if(editBirth)output.Append(",\"birthDate\":").Append(Json(birthday));
                if(editYearsPro)output.Append(",\"yearsPro\":").Append(yearsPro.Value);
                if (editAudio) output.Append(",\"fullNameId\":").Append(fullNameId);
                if (editFrom) output.Append(",\"fromIndex\":").Append(fromIndex).Append(",\"fromName\":").Append(Json(fromName));
                output.Append(",\"recordAddress\":").Append(Json("0x" + playerAddress.ToString("X")));
                output.Append(",\"readBackVerified\":true,\"rollbackOnFailure\":true}");
                Console.OutputEncoding = new UTF8Encoding(false);
                Console.WriteLine(output.ToString());
                return 0;
            }
        }

        private static int RunTeamEdit(string[] args)
        {
            int pid = ProcessIdFromArgs(args);
            int recordIndex = Int32.Parse(Argument(args, "--record-index"), CultureInfo.InvariantCulture);
            ushort teamId = UInt16.Parse(Argument(args, "--team-id"), CultureInfo.InvariantCulture);
            string expectedName = Argument(args, "--expected-name");
            string teamName = Argument(args, "--name");
            string cityName = Argument(args, "--city-name");
            string cityShort = Argument(args, "--city-short");
            string stateShort = Argument(args, "--state-short");
            string mainLogo = Argument(args, "--main-logo");
            int rosterType = Int32.Parse(Argument(args, "--roster-type"), CultureInfo.InvariantCulture);
            int historicYear = Int32.Parse(Argument(args, "--historic-year"), CultureInfo.InvariantCulture);
            ushort arenaId = UInt16.Parse(Argument(args, "--arena-id"), CultureInfo.InvariantCulture);
            if (rosterType < 0 || rosterType > 31) throw new InvalidOperationException("Roster type must be between 0 and 31.");
            if (historicYear < 0 || historicYear > 127) throw new InvalidOperationException("Historic year must be between 0 and 127.");

            using (Reader memory = new Reader(pid, true))
            {
                long regionBase, regionSize, playerTable;
                int playerCount;
                if (!LocateDatabase(memory, out regionBase, out regionSize, out playerCount, out playerTable))
                    throw new InvalidOperationException("A unique validated NBA 2K27 database was not found.");
                byte[] teamHeader = memory.Read(regionBase + TeamCountOffset, 16);
                int teamCount = checked((int)U64(teamHeader, 0));
                long teamTable = checked((long)U64(teamHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, teamTable, teamCount, TeamStride, 1024) || recordIndex < 0 || recordIndex >= teamCount)
                    throw new InvalidOperationException("The team directory or requested record failed structural validation.");
                long teamAddress = teamTable + (long)recordIndex * TeamStride;
                byte[] record = memory.Read(teamAddress, TeamStride);
                string currentName = DecodeUtf16(record, TeamNameOffset, 48);
                if (BitConverter.ToUInt16(record, TeamUniqueIdOffset) != teamId || currentName != expectedName)
                    throw new InvalidOperationException("The selected team changed before Save; refresh the team list and try again.");

                byte[] arenaHeader = memory.Read(regionBase + ArenaCountOffset, 16);
                int arenaCount = checked((int)U64(arenaHeader, 0));
                long arenaTable = checked((long)U64(arenaHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, arenaTable, arenaCount, ArenaStride, 2048))
                    throw new InvalidOperationException("The arena directory failed structural validation.");
                long selectedArenaAddress = 0;
                int arenaMatches = 0;
                for (int index = 0; index < arenaCount; index++)
                {
                    long address = arenaTable + (long)index * ArenaStride;
                    ushort candidate = BitConverter.ToUInt16(memory.Read(address + ArenaIdOffset, 2), 0);
                    if (candidate == arenaId) { selectedArenaAddress = address; arenaMatches++; }
                }
                if (arenaMatches != 1) throw new InvalidOperationException("Arena ID " + arenaId + " did not resolve to one exact live arena record.");

                long cloneAddress=0;byte[] cloneBefore=null,cloneAfter=null;
                bool geometry=Array.IndexOf(args,"--floor-file")>=0;
                if(geometry){
                    string floor=Argument(args,"--floor-file");FixedAscii(floor,40,"Floor asset stem");
                    long catalogAddress;var arenaCatalog=ReadArenas(memory,regionBase,regionSize,out catalogAddress);
                    var donor=arenaCatalog.Find(a=>a.Address==selectedArenaAddress);
                    var used=new HashSet<long>();
                    for(int i=0;i<teamCount;i++)used.Add(BitConverter.ToInt64(memory.Read(teamTable+(long)i*TeamStride+TeamArenaPointerOffset,8),0));
                    var target=TeamArenaPair.Target(donor,floor,arenaCatalog,used);
                    if(target.Address!=donor.Address){
                        cloneAddress=target.Address;cloneBefore=memory.Read(cloneAddress,ArenaStride);cloneAfter=memory.Read(donor.Address,ArenaStride);
                        Buffer.BlockCopy(FixedAscii(floor,40,"Floor asset stem"),0,cloneAfter,ArenaFloorOffset,40);
                        Buffer.BlockCopy(BitConverter.GetBytes(target.Id),0,cloneAfter,ArenaIdOffset,2);
                        selectedArenaAddress=target.Address;arenaId=target.Id;
                    }
                }

                byte rosterByte = record[TeamRosterTypeOffset];
                rosterByte = (byte)((rosterByte & ~(0x1F << 2)) | (rosterType << 2));
                ushort historicPacked = BitConverter.ToUInt16(record, TeamHistoricYearOffset);
                historicPacked = (ushort)((historicPacked & ~(0x7F << 3)) | (historicYear << 3));
                List<WriteOperation> operations = new List<WriteOperation> {
                    Operation(record, TeamNameOffset, FixedUtf16(teamName, 48, "Team name")),
                    Operation(record, TeamCityOffset, FixedUtf16(cityName, 36, "City name")),
                    Operation(record, TeamCityShortOffset, FixedUtf16(cityShort, 14, "City abbreviation")),
                    Operation(record, TeamStateShortOffset, FixedUtf16(stateShort, 14, "State abbreviation")),
                    Operation(record, TeamMainLogoOffset, FixedUtf16(mainLogo, 72, "Main logo")),
                    Operation(record, TeamRosterTypeOffset, new byte[] { rosterByte }),
                    Operation(record, TeamHistoricYearOffset, BitConverter.GetBytes(historicPacked)),
                    Operation(record, TeamArenaPointerOffset, BitConverter.GetBytes(selectedArenaAddress))
                };
                List<WriteOperation> applied = new List<WriteOperation>();
                byte[] expectedTeam=(byte[])record.Clone();bool cloned=false;
                // Recovery is persisted before any geometry or team write.
                if(geometry){
                    string directory=System.IO.Path.GetFullPath(Argument(args,"--backup-root"));System.IO.Directory.CreateDirectory(directory);
                    string filename=System.IO.Path.Combine(directory,"team-"+Guid.NewGuid().ToString("N")+".json");
                    byte[] recovery=Encoding.UTF8.GetBytes(new System.Web.Script.Serialization.JavaScriptSerializer().Serialize(new {schema="nba2k27-team-recovery-v1",pid=pid,teamId=teamId,recordIndex=recordIndex,teamBefore=Convert.ToBase64String(record),cloneAddress=cloneAddress,cloneBefore=cloneBefore==null?null:Convert.ToBase64String(cloneBefore),cloneAfter=cloneAfter==null?null:Convert.ToBase64String(cloneAfter)}));
                    using(var stream=new System.IO.FileStream(filename,System.IO.FileMode.CreateNew,System.IO.FileAccess.Write,System.IO.FileShare.None)){stream.Write(recovery,0,recovery.Length);stream.Flush(true);}
                }
                try
                {
                    if(!SameBytes(teamHeader,memory.Read(regionBase+TeamCountOffset,16))||!SameBytes(arenaHeader,memory.Read(regionBase+ArenaCountOffset,16))||!SameBytes(record,memory.Read(teamAddress,TeamStride)))throw new Exception("The team or arena directory changed before Save.");
                    if(cloneAddress!=0){
                        for(int i=0;i<teamCount;i++)if(BitConverter.ToInt64(memory.Read(teamTable+(long)i*TeamStride+TeamArenaPointerOffset,8),0)==cloneAddress)throw new Exception("The reserved arena slot is now referenced by a team.");
                        if(!SameBytes(cloneBefore,memory.Read(cloneAddress,ArenaStride)))throw new Exception("The reserved arena slot changed.");
                        cloned=true;memory.Write(cloneAddress,cloneAfter);if(!SameBytes(cloneAfter,memory.Read(cloneAddress,ArenaStride)))throw new Exception("Arena/floor clone verification failed.");
                    }
                    foreach (WriteOperation operation in operations)
                    {
                        if(!SameBytes(teamHeader,memory.Read(regionBase+TeamCountOffset,16))||!SameBytes(arenaHeader,memory.Read(regionBase+ArenaCountOffset,16))||!SameBytes(expectedTeam,memory.Read(teamAddress,TeamStride)))throw new Exception("The team changed during Save.");
                        applied.Add(operation);
                        memory.Write(teamAddress + operation.Offset, operation.After);
                        Buffer.BlockCopy(operation.After,0,expectedTeam,operation.Offset,operation.After.Length);
                        byte[] verified = memory.Read(teamAddress + operation.Offset, operation.After.Length);
                        if (!SameBytes(verified, operation.After)) throw new InvalidOperationException("A team-field read-back check failed.");
                    }
                    byte[] identity = memory.Read(teamAddress, TeamStride);
                    if (BitConverter.ToUInt16(identity, TeamUniqueIdOffset) != teamId||!SameBytes(identity,expectedTeam)||!SameBytes(teamHeader,memory.Read(regionBase+TeamCountOffset,16))||!SameBytes(arenaHeader,memory.Read(regionBase+ArenaCountOffset,16)))
                        throw new InvalidOperationException("The team identity changed during Save.");
                }
                catch (Exception error)
                {
                    bool rollbackPassed = true;
                    for (int index = applied.Count - 1; index >= 0; index--)
                    {
                        try
                        {
                            if(!SameBytes(teamHeader,memory.Read(regionBase+TeamCountOffset,16))||!SameBytes(arenaHeader,memory.Read(regionBase+ArenaCountOffset,16))||BitConverter.ToUInt16(memory.Read(teamAddress+TeamUniqueIdOffset,2),0)!=teamId)throw new Exception("The owner changed; recovery write refused.");
                            byte[] current=memory.Read(teamAddress+applied[index].Offset,applied[index].Before.Length);
                            if(!SameBytes(current,applied[index].After)&&!SameBytes(current,applied[index].Before))throw new Exception("A foreign team edit prevents recovery.");
                            memory.Write(teamAddress + applied[index].Offset, applied[index].Before);
                            rollbackPassed &= SameBytes(memory.Read(teamAddress + applied[index].Offset, applied[index].Before.Length), applied[index].Before);
                        }
                        catch (Exception) { rollbackPassed = false; }
                    }
                    if(cloned)try{
                        if(!SameBytes(arenaHeader,memory.Read(regionBase+ArenaCountOffset,16))||!SameBytes(cloneAfter,memory.Read(cloneAddress,ArenaStride)))throw new Exception("The arena changed; recovery refused.");
                        for(int i=0;i<teamCount;i++)if(BitConverter.ToInt64(memory.Read(teamTable+(long)i*TeamStride+TeamArenaPointerOffset,8),0)==cloneAddress)throw new Exception("The clone is still referenced; recovery refused.");
                        memory.Write(cloneAddress,cloneBefore);rollbackPassed&=SameBytes(cloneBefore,memory.Read(cloneAddress,ArenaStride));
                    }catch{rollbackPassed=false;}
                    throw new InvalidOperationException(error.Message + (rollbackPassed ? " All applied fields were rolled back." : " Rollback verification failed."));
                }
                StringBuilder output = new StringBuilder();
                output.Append("{\"schema\":\"nba2k27-team-edit-v1\",\"ok\":true");
                output.Append(",\"pid\":").Append(pid).Append(",\"gameCodeHooks\":0,\"writes\":").Append(operations.Count+(cloned?1:0));
                output.Append(",\"recordIndex\":").Append(recordIndex).Append(",\"teamId\":").Append(teamId);
                output.Append(",\"recordAddress\":").Append(Json("0x" + teamAddress.ToString("X")));
                output.Append(",\"arenaId\":").Append(arenaId).Append(",\"readBackVerified\":true,\"rollbackOnFailure\":true}");
                Console.OutputEncoding = new UTF8Encoding(false);
                Console.WriteLine(output.ToString());
                return 0;
            }
        }

        private static int RunTeamUniformEdit(string[] args)
        {
            int pid = ProcessIdFromArgs(args);
            int recordIndex = Int32.Parse(Argument(args, "--record-index"), CultureInfo.InvariantCulture);
            ushort teamId = UInt16.Parse(Argument(args, "--team-id"), CultureInfo.InvariantCulture);
            string expectedTeamName = Argument(args, "--expected-team-name");
            int slot = Int32.Parse(Argument(args, "--slot"), CultureInfo.InvariantCulture);
            ushort uniformId = UInt16.Parse(Argument(args, "--uniform-id"), CultureInfo.InvariantCulture);
            string expectedBaseFilename = Argument(args, "--expected-base-filename");
            int expectedHome = Int32.Parse(Argument(args, "--expected-home"), CultureInfo.InvariantCulture);
            int expectedAway = Int32.Parse(Argument(args, "--expected-away"), CultureInfo.InvariantCulture);
            int expectedLogo = Int32.Parse(Argument(args, "--expected-logo"), CultureInfo.InvariantCulture);
            int home = Int32.Parse(Argument(args, "--home"), CultureInfo.InvariantCulture);
            int away = Int32.Parse(Argument(args, "--away"), CultureInfo.InvariantCulture);
            int logo = Int32.Parse(Argument(args, "--logo"), CultureInfo.InvariantCulture);
            string[] colorNames = { "uniform-primary", "uniform-secondary", "primary", "secondary", "tertiary", "quaternary", "quintary", "sextary", "primary-accessory", "secondary-accessory", "tertiary-accessory", "quaternary-accessory", "quintary-accessory", "undershirt-home", "undershirt-away", "away-shoe-primary", "primary-sock", "secondary-sock", "tertiary-sock", "quaternary-sock", "quintary-sock" };
            int[] colorOffsets = { 0x10C, 0x110, 0x114, 0x118, 0x11C, 0x120, 0x124, 0x128, 0x12C, 0x130, 0x134, 0x138, 0x13C, 0x140, 0x144, 0x148, 0x14C, 0x150, 0x154, 0x158, 0x15C };
            uint[] expectedColors = new uint[colorNames.Length];
            uint[] colors = new uint[colorNames.Length];
            for (int index = 0; index < colorNames.Length; index++)
            {
                expectedColors[index] = UInt32.Parse(Argument(args, "--expected-" + colorNames[index]), CultureInfo.InvariantCulture);
                colors[index] = UInt32.Parse(Argument(args, "--" + colorNames[index]), CultureInfo.InvariantCulture);
            }
            uint expectedFlags160 = UInt32.Parse(Argument(args, "--expected-flags-160"), CultureInfo.InvariantCulture);
            uint expectedFlags164 = UInt32.Parse(Argument(args, "--expected-flags-164"), CultureInfo.InvariantCulture);
            uint expectedFlags168 = UInt32.Parse(Argument(args, "--expected-flags-168"), CultureInfo.InvariantCulture);
            uint expectedFlags16C = UInt32.Parse(Argument(args, "--expected-flags-16c"), CultureInfo.InvariantCulture);
            int type = Int32.Parse(Argument(args, "--type"), CultureInfo.InvariantCulture);
            int logoType = Int32.Parse(Argument(args, "--logo-type"), CultureInfo.InvariantCulture);
            int logoBrand = Int32.Parse(Argument(args, "--logo-brand"), CultureInfo.InvariantCulture);
            int dominantColor = Int32.Parse(Argument(args, "--dominant-color"), CultureInfo.InvariantCulture);
            int scorebugUniformColor = Int32.Parse(Argument(args, "--scorebug-uniform-color"), CultureInfo.InvariantCulture);
            int disablePhoto = Int32.Parse(Argument(args, "--disable-photo"), CultureInfo.InvariantCulture);
            int useTeamLogo = Int32.Parse(Argument(args, "--use-team-logo"), CultureInfo.InvariantCulture);
            int homePriority = Int32.Parse(Argument(args, "--home-priority"), CultureInfo.InvariantCulture);
            int awayPriority = Int32.Parse(Argument(args, "--away-priority"), CultureInfo.InvariantCulture);
            int topNameplate = Int32.Parse(Argument(args, "--top-nameplate"), CultureInfo.InvariantCulture);
            int bottomNameplate = Int32.Parse(Argument(args, "--bottom-nameplate"), CultureInfo.InvariantCulture);
            int side = Int32.Parse(Argument(args, "--side"), CultureInfo.InvariantCulture);
            int overrideNbaPatch = Int32.Parse(Argument(args, "--override-nba-patch"), CultureInfo.InvariantCulture);
            int numberOnShorts = Int32.Parse(Argument(args, "--number-on-shorts"), CultureInfo.InvariantCulture);
            int alternate = Int32.Parse(Argument(args, "--alternate"), CultureInfo.InvariantCulture);
            int primaryLogoSock = Int32.Parse(Argument(args, "--primary-logo-sock"), CultureInfo.InvariantCulture);
            int secondaryLogoSock = Int32.Parse(Argument(args, "--secondary-logo-sock"), CultureInfo.InvariantCulture);
            int tertiaryLogoSock = Int32.Parse(Argument(args, "--tertiary-logo-sock"), CultureInfo.InvariantCulture);
            int quaternaryLogoSock = Int32.Parse(Argument(args, "--quaternary-logo-sock"), CultureInfo.InvariantCulture);
            int quintaryLogoSock = Int32.Parse(Argument(args, "--quintary-logo-sock"), CultureInfo.InvariantCulture);
            int rightAlignFrontNumber = Int32.Parse(Argument(args, "--right-align-front-number"), CultureInfo.InvariantCulture);
            int legHome = Int32.Parse(Argument(args, "--leg-home"), CultureInfo.InvariantCulture);
            int legAway = Int32.Parse(Argument(args, "--leg-away"), CultureInfo.InvariantCulture);
            int armHome = Int32.Parse(Argument(args, "--arm-home"), CultureInfo.InvariantCulture);
            int armAway = Int32.Parse(Argument(args, "--arm-away"), CultureInfo.InvariantCulture);
            int sockHome = Int32.Parse(Argument(args, "--sock-home"), CultureInfo.InvariantCulture);
            int sockAway = Int32.Parse(Argument(args, "--sock-away"), CultureInfo.InvariantCulture);
            int designation = Int32.Parse(Argument(args, "--designation"), CultureInfo.InvariantCulture);
            if (slot < 0 || slot >= TeamUniformSlotCount) throw new InvalidOperationException("Uniform slot must be between 0 and 31.");
            if (expectedHome < 0 || expectedHome > 7 || expectedAway < 0 || expectedAway > 7
                || home < 0 || home > 7 || away < 0 || away > 7)
                throw new InvalidOperationException("Headband colors must be between 0 and 7.");
            if (expectedLogo < 0 || expectedLogo > 3 || logo < 0 || logo > 3)
                throw new InvalidOperationException("Headband logo type must be between 0 and 3.");
            int[] booleans = { scorebugUniformColor, disablePhoto, useTeamLogo, overrideNbaPatch, numberOnShorts, alternate, primaryLogoSock, secondaryLogoSock, tertiaryLogoSock, quaternaryLogoSock, quintaryLogoSock, rightAlignFrontNumber };
            foreach (int value in booleans) if (value < 0 || value > 1) throw new InvalidOperationException("Uniform Boolean settings must be 0 or 1.");
            if (type < 0 || type > 20 || logoType < 0 || logoType > 5 || logoBrand < 0 || logoBrand > 3
                || dominantColor < 0 || dominantColor > 20 || homePriority < 0 || homePriority > 15
                || awayPriority < 0 || awayPriority > 15 || topNameplate < 0 || topNameplate > 3
                || bottomNameplate < 0 || bottomNameplate > 3 || side < 0 || side > 1
                || legHome < 0 || legHome > 7 || legAway < 0 || legAway > 7
                || armHome < 0 || armHome > 7 || armAway < 0 || armAway > 7
                || sockHome < 0 || sockHome > 7 || sockAway < 0 || sockAway > 7
                || designation < 0 || designation > 10)
                throw new InvalidOperationException("One or more packed uniform settings are outside their verified enum range.");

            using (Reader memory = new Reader(pid, true))
            {
                long regionBase, regionSize, playerTable;
                int playerCount;
                if (!LocateDatabase(memory, out regionBase, out regionSize, out playerCount, out playerTable))
                    throw new InvalidOperationException("A unique validated NBA 2K27 database was not found.");

                byte[] teamHeader = memory.Read(regionBase + TeamCountOffset, 16);
                int teamCount = checked((int)U64(teamHeader, 0));
                long teamTable = checked((long)U64(teamHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, teamTable, teamCount, TeamStride, 1024)
                    || recordIndex < 0 || recordIndex >= teamCount)
                    throw new InvalidOperationException("The team directory or requested record failed structural validation.");
                long teamAddress = teamTable + (long)recordIndex * TeamStride;
                byte[] teamRecord = memory.Read(teamAddress, TeamStride);
                if (BitConverter.ToUInt16(teamRecord, TeamUniqueIdOffset) != teamId
                    || DecodeUtf16(teamRecord, TeamNameOffset, 48) != expectedTeamName)
                    throw new InvalidOperationException("The selected team changed before Save; refresh the team list and try again.");

                byte[] uniformHeader = memory.Read(regionBase + UniformCountOffset, 16);
                int uniformCount = checked((int)U64(uniformHeader, 0));
                long uniformTable = checked((long)U64(uniformHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, uniformTable, uniformCount, UniformStride, 8192))
                    throw new InvalidOperationException("The uniform directory failed structural validation.");
                long uniformAddress = checked((long)U64(teamRecord, TeamUniformSlotOffset + slot * 8));
                long uniformDifference = uniformAddress - uniformTable;
                if (uniformAddress < uniformTable || uniformDifference % UniformStride != 0
                    || uniformDifference / UniformStride < 0 || uniformDifference / UniformStride >= uniformCount)
                    throw new InvalidOperationException("The selected team slot no longer owns one exact live uniform record.");

                byte[] uniformRecord = memory.Read(uniformAddress, UniformStride);
                uint original160 = BitConverter.ToUInt32(uniformRecord, 0x160);
                uint original164 = BitConverter.ToUInt32(uniformRecord, 0x164);
                uint original168 = BitConverter.ToUInt32(uniformRecord, 0x168);
                uint original16C = BitConverter.ToUInt32(uniformRecord, 0x16C);
                if (BitConverter.ToUInt16(uniformRecord, 0x50) != uniformId
                    || BitConverter.ToUInt16(uniformRecord, 0x52) != teamId
                    || DecodeAscii(uniformRecord, 0x08, 0x40) != expectedBaseFilename)
                    throw new InvalidOperationException("The selected uniform changed before Save; refresh the team and try again.");
                if (original160 != expectedFlags160 || original164 != expectedFlags164
                    || original168 != expectedFlags168 || original16C != expectedFlags16C)
                    throw new InvalidOperationException("The packed uniform settings changed before Save; refresh the team and try again.");
                if (((original16C >> 12) & 7) != expectedHome || ((original16C >> 15) & 7) != expectedAway
                    || ((original16C >> 18) & 7) != expectedLogo)
                    throw new InvalidOperationException("The uniform headband settings changed before Save; refresh the team and try again.");
                for (int index = 0; index < colorOffsets.Length; index++)
                    if (BitConverter.ToUInt32(uniformRecord, colorOffsets[index]) != expectedColors[index])
                        throw new InvalidOperationException("The uniform palette changed before Save; refresh the team and try again.");

                const uint editableMask160 = 0xFFu | (0xFu << 16) | (0xFu << 20) | (0x1Fu << 24) | (1u << 29);
                uint updated160 = (original160 & ~editableMask160) | (uint)type | ((uint)logoType << 16)
                    | ((uint)logoBrand << 20) | ((uint)dominantColor << 24) | ((uint)scorebugUniformColor << 29);
                const uint editableMask164 = (1u << 30) | (1u << 31);
                uint updated164 = (original164 & ~editableMask164) | ((uint)disablePhoto << 30) | ((uint)useTeamLogo << 31);
                const uint editableMask168 = (0xFu << 5) | (0xFu << 9) | (3u << 13) | (3u << 15) | (1u << 17)
                    | (1u << 19) | (1u << 21) | (1u << 22) | (0x1Fu << 23) | (1u << 28);
                uint updated168 = (original168 & ~editableMask168) | ((uint)homePriority << 5) | ((uint)awayPriority << 9)
                    | ((uint)topNameplate << 13) | ((uint)bottomNameplate << 15) | ((uint)side << 17)
                    | ((uint)overrideNbaPatch << 19) | ((uint)numberOnShorts << 21) | ((uint)alternate << 22)
                    | ((uint)primaryLogoSock << 23) | ((uint)secondaryLogoSock << 24) | ((uint)tertiaryLogoSock << 25)
                    | ((uint)quaternaryLogoSock << 26) | ((uint)quintaryLogoSock << 27) | ((uint)rightAlignFrontNumber << 28);
                const uint editableMask16C = 0xF7FFFFFFu;
                uint updated16C = (original16C & ~editableMask16C) | (uint)legHome | ((uint)legAway << 3)
                    | ((uint)armHome << 6) | ((uint)armAway << 9) | ((uint)home << 12) | ((uint)away << 15)
                    | ((uint)logo << 18) | ((uint)sockHome << 21) | ((uint)sockAway << 24) | ((uint)designation << 28);
                List<WriteOperation> operations = new List<WriteOperation>();
                for (int index = 0; index < colorOffsets.Length; index++)
                    operations.Add(Operation(uniformRecord, colorOffsets[index], BitConverter.GetBytes(colors[index])));
                operations.Add(Operation(uniformRecord, 0x160, BitConverter.GetBytes(updated160)));
                operations.Add(Operation(uniformRecord, 0x164, BitConverter.GetBytes(updated164)));
                operations.Add(Operation(uniformRecord, 0x168, BitConverter.GetBytes(updated168)));
                operations.Add(Operation(uniformRecord, 0x16C, BitConverter.GetBytes(updated16C)));
                List<WriteOperation> applied = new List<WriteOperation>();
                try
                {
                    foreach (WriteOperation operation in operations)
                    {
                        if (SameBytes(operation.Before, operation.After)) continue;
                        memory.Write(uniformAddress + operation.Offset, operation.After);
                        applied.Add(operation);
                        if (!SameBytes(memory.Read(uniformAddress + operation.Offset, operation.After.Length), operation.After))
                            throw new InvalidOperationException("A uniform palette read-back check failed.");
                    }
                    byte[] finalTeam = memory.Read(teamAddress, TeamStride);
                    byte[] finalUniform = memory.Read(uniformAddress, UniformStride);
                    if (BitConverter.ToUInt16(finalTeam, TeamUniqueIdOffset) != teamId
                        || DecodeUtf16(finalTeam, TeamNameOffset, 48) != expectedTeamName
                        || checked((long)U64(finalTeam, TeamUniformSlotOffset + slot * 8)) != uniformAddress
                        || BitConverter.ToUInt16(finalUniform, 0x50) != uniformId
                        || BitConverter.ToUInt16(finalUniform, 0x52) != teamId
                        || DecodeAscii(finalUniform, 0x08, 0x40) != expectedBaseFilename
                        || BitConverter.ToUInt32(finalUniform, 0x160) != updated160
                        || BitConverter.ToUInt32(finalUniform, 0x164) != updated164
                        || BitConverter.ToUInt32(finalUniform, 0x168) != updated168
                        || BitConverter.ToUInt32(finalUniform, 0x16C) != updated16C)
                        throw new InvalidOperationException("The uniform ownership or read-back verification failed.");
                    for (int index = 0; index < colorOffsets.Length; index++)
                        if (BitConverter.ToUInt32(finalUniform, colorOffsets[index]) != colors[index])
                            throw new InvalidOperationException("The final uniform palette verification failed.");
                }
                catch (Exception error)
                {
                    bool rollbackPassed = true;
                    for (int index = applied.Count - 1; index >= 0; index--)
                    {
                        try
                        {
                            WriteOperation operation = applied[index];
                            if (!SameBytes(memory.Read(uniformAddress + operation.Offset, operation.After.Length), operation.After))
                            { rollbackPassed = false; continue; }
                            memory.Write(uniformAddress + operation.Offset, operation.Before);
                            rollbackPassed &= SameBytes(memory.Read(uniformAddress + operation.Offset, operation.Before.Length), operation.Before);
                        }
                        catch (Exception) { rollbackPassed = false; }
                    }
                    throw new InvalidOperationException(error.Message + (rollbackPassed ? " The original uniform settings were restored." : " Rollback verification failed."));
                }

                StringBuilder output = new StringBuilder();
                output.Append("{\"schema\":\"nba2k27-team-uniform-edit-v4\",\"ok\":true");
                output.Append(",\"pid\":").Append(pid).Append(",\"gameCodeHooks\":0,\"writes\":52");
                output.Append(",\"physicalWrites\":").Append(applied.Count);
                output.Append(",\"recordIndex\":").Append(recordIndex).Append(",\"teamId\":").Append(teamId);
                output.Append(",\"slot\":").Append(slot).Append(",\"uniformId\":").Append(uniformId);
                output.Append(",\"recordAddress\":").Append(Json("0x" + uniformAddress.ToString("X")));
                output.Append(",\"readBackVerified\":true,\"rollbackOnFailure\":true}");
                Console.OutputEncoding = new UTF8Encoding(false);
                Console.WriteLine(output.ToString());
                return 0;
            }
        }

        private static int RunTeamUniformLinkTest(string[] args)
        {
            bool persist = Array.IndexOf(args, "--link-team-uniform") >= 0;
            int pid = ProcessIdFromArgs(args);
            int recordIndex = Int32.Parse(Argument(args, "--record-index"), CultureInfo.InvariantCulture);
            ushort teamId = UInt16.Parse(Argument(args, "--team-id"), CultureInfo.InvariantCulture);
            string expectedTeamName = Argument(args, "--expected-team-name");
            int slot = Int32.Parse(Argument(args, "--slot"), CultureInfo.InvariantCulture);
            int uniformRecordIndex = Int32.Parse(Argument(args, "--uniform-record-index"), CultureInfo.InvariantCulture);
            ushort uniformId = UInt16.Parse(Argument(args, "--uniform-id"), CultureInfo.InvariantCulture);
            string expectedBaseFilename = Argument(args, "--expected-base-filename");
            if (slot < 0 || slot >= TeamUniformSlotCount)
                throw new InvalidOperationException("Uniform slot must be between 0 and 31.");

            using (Reader memory = new Reader(pid, true))
            {
                long regionBase, regionSize, playerTable;
                int playerCount;
                if (!LocateDatabase(memory, out regionBase, out regionSize, out playerCount, out playerTable))
                    throw new InvalidOperationException("A unique validated NBA 2K27 database was not found.");

                byte[] teamHeader = memory.Read(regionBase + TeamCountOffset, 16);
                int teamCount = checked((int)U64(teamHeader, 0));
                long teamTable = checked((long)U64(teamHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, teamTable, teamCount, TeamStride, 1024)
                    || recordIndex < 0 || recordIndex >= teamCount)
                    throw new InvalidOperationException("The team directory or requested record failed structural validation.");
                long teamAddress = teamTable + (long)recordIndex * TeamStride;
                byte[] teamRecord = memory.Read(teamAddress, TeamStride);
                if (BitConverter.ToUInt16(teamRecord, TeamUniqueIdOffset) != teamId
                    || DecodeUtf16(teamRecord, TeamNameOffset, 48) != expectedTeamName)
                    throw new InvalidOperationException("The selected team changed before the link test; refresh and try again.");

                long slotAddress = teamAddress + TeamUniformSlotOffset + slot * 8L;
                byte[] originalSlot = memory.Read(slotAddress, 8);
                if (U64(originalSlot, 0) != 0)
                    throw new InvalidOperationException("The selected uniform slot is no longer empty.");

                byte[] uniformHeader = memory.Read(regionBase + UniformCountOffset, 16);
                int uniformCount = checked((int)U64(uniformHeader, 0));
                long uniformTable = checked((long)U64(uniformHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, uniformTable, uniformCount, UniformStride, 8192)
                    || uniformRecordIndex < 0 || uniformRecordIndex >= uniformCount)
                    throw new InvalidOperationException("The uniform directory or requested record failed structural validation.");
                long uniformAddress = uniformTable + (long)uniformRecordIndex * UniformStride;
                byte[] uniformRecord = memory.Read(uniformAddress, UniformStride);
                if (BitConverter.ToUInt16(uniformRecord, 0x50) != uniformId
                    || BitConverter.ToUInt16(uniformRecord, 0x52) != teamId
                    || DecodeAscii(uniformRecord, 0x08, 0x40) != expectedBaseFilename)
                    throw new InvalidOperationException("The candidate uniform changed before the link test; refresh and try again.");

                for (int teamIndex = 0; teamIndex < teamCount; teamIndex++)
                {
                    long candidateTeamAddress = teamTable + (long)teamIndex * TeamStride;
                    byte[] pointers = memory.Read(candidateTeamAddress + TeamUniformSlotOffset, TeamUniformSlotCount * 8);
                    for (int candidateSlot = 0; candidateSlot < TeamUniformSlotCount; candidateSlot++)
                        if (checked((long)U64(pointers, candidateSlot * 8)) == uniformAddress)
                            throw new InvalidOperationException("The candidate uniform is already referenced by another team slot.");
                }

                byte[] linkedSlot = BitConverter.GetBytes(uniformAddress);
                bool applied = false;
                bool restored = false;
                Exception failure = null;
                try
                {
                    memory.Write(slotAddress, linkedSlot);
                    applied = true;
                    if (!SameBytes(memory.Read(slotAddress, 8), linkedSlot))
                        throw new InvalidOperationException("The temporary uniform link did not read back.");
                    byte[] finalUniform = memory.Read(uniformAddress, UniformStride);
                    if (BitConverter.ToUInt16(finalUniform, 0x50) != uniformId
                        || BitConverter.ToUInt16(finalUniform, 0x52) != teamId
                        || DecodeAscii(finalUniform, 0x08, 0x40) != expectedBaseFilename)
                        throw new InvalidOperationException("The candidate uniform identity changed during the link test.");
                }
                catch (Exception error) { failure = error; }
                finally
                {
                    if (applied && (failure != null || !persist) && SameBytes(memory.Read(slotAddress, 8), linkedSlot))
                    {
                        memory.Write(slotAddress, originalSlot);
                        restored = SameBytes(memory.Read(slotAddress, 8), originalSlot);
                    }
                }
                if (failure != null)
                    throw new InvalidOperationException(failure.Message + (restored ? " The empty slot was restored." : " Slot restoration failed."));
                if (!persist && !restored) throw new InvalidOperationException("The temporary uniform link passed, but slot restoration failed.");

                StringBuilder output = new StringBuilder();
                output.Append("{\"schema\":").Append(Json(persist ? "nba2k27-team-uniform-link-v1" : "nba2k27-team-uniform-link-test-v1")).Append(",\"ok\":true");
                output.Append(",\"pid\":").Append(pid).Append(",\"gameCodeHooks\":0,\"physicalWrites\":").Append(persist ? 1 : 2);
                output.Append(",\"recordIndex\":").Append(recordIndex).Append(",\"teamId\":").Append(teamId);
                output.Append(",\"slot\":").Append(slot).Append(",\"uniformRecordIndex\":").Append(uniformRecordIndex);
                output.Append(",\"uniformId\":").Append(uniformId).Append(",\"baseFilename\":").Append(Json(expectedBaseFilename));
                output.Append(",\"linkReadBackVerified\":true,\"restoredToNull\":").Append(restored ? "true" : "false").Append(",\"rollbackOnFailure\":true}");
                Console.OutputEncoding = new UTF8Encoding(false);
                Console.WriteLine(output.ToString());
                return 0;
            }
        }

        private static int RunTeamUniformUnlink(string[] args)
        {
            int pid = ProcessIdFromArgs(args);
            int recordIndex = Int32.Parse(Argument(args, "--record-index"), CultureInfo.InvariantCulture);
            ushort teamId = UInt16.Parse(Argument(args, "--team-id"), CultureInfo.InvariantCulture);
            string expectedTeamName = Argument(args, "--expected-team-name");
            int slot = Int32.Parse(Argument(args, "--slot"), CultureInfo.InvariantCulture);
            int uniformRecordIndex = Int32.Parse(Argument(args, "--uniform-record-index"), CultureInfo.InvariantCulture);
            ushort uniformId = UInt16.Parse(Argument(args, "--uniform-id"), CultureInfo.InvariantCulture);
            string expectedBaseFilename = Argument(args, "--expected-base-filename");
            if (slot < 0 || slot >= TeamUniformSlotCount)
                throw new InvalidOperationException("Uniform slot must be between 0 and 31.");

            using (Reader memory = new Reader(pid, true))
            {
                long regionBase, regionSize, playerTable;
                int playerCount;
                if (!LocateDatabase(memory, out regionBase, out regionSize, out playerCount, out playerTable))
                    throw new InvalidOperationException("A unique validated NBA 2K27 database was not found.");
                byte[] teamHeader = memory.Read(regionBase + TeamCountOffset, 16);
                int teamCount = checked((int)U64(teamHeader, 0));
                long teamTable = checked((long)U64(teamHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, teamTable, teamCount, TeamStride, 1024)
                    || recordIndex < 0 || recordIndex >= teamCount)
                    throw new InvalidOperationException("The team directory or requested record failed structural validation.");
                long teamAddress = teamTable + (long)recordIndex * TeamStride;
                byte[] teamRecord = memory.Read(teamAddress, TeamStride);
                if (BitConverter.ToUInt16(teamRecord, TeamUniqueIdOffset) != teamId
                    || DecodeUtf16(teamRecord, TeamNameOffset, 48) != expectedTeamName)
                    throw new InvalidOperationException("The selected team changed before removal; refresh and try again.");

                byte[] uniformHeader = memory.Read(regionBase + UniformCountOffset, 16);
                int uniformCount = checked((int)U64(uniformHeader, 0));
                long uniformTable = checked((long)U64(uniformHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, uniformTable, uniformCount, UniformStride, 8192)
                    || uniformRecordIndex < 0 || uniformRecordIndex >= uniformCount)
                    throw new InvalidOperationException("The uniform directory or requested record failed structural validation.");
                long uniformAddress = uniformTable + (long)uniformRecordIndex * UniformStride;
                byte[] uniformRecord = memory.Read(uniformAddress, UniformStride);
                if (BitConverter.ToUInt16(uniformRecord, 0x50) != uniformId
                    || BitConverter.ToUInt16(uniformRecord, 0x52) != teamId
                    || DecodeAscii(uniformRecord, 0x08, 0x40) != expectedBaseFilename)
                    throw new InvalidOperationException("The linked uniform identity changed before removal.");

                long slotAddress = teamAddress + TeamUniformSlotOffset + slot * 8L;
                byte[] linkedSlot = memory.Read(slotAddress, 8);
                if (checked((long)U64(linkedSlot, 0)) != uniformAddress)
                    throw new InvalidOperationException("The selected team slot no longer points to this uniform.");
                int referenceCount = 0;
                for (int teamIndex = 0; teamIndex < teamCount; teamIndex++)
                {
                    long candidateTeamAddress = teamTable + (long)teamIndex * TeamStride;
                    byte[] pointers = memory.Read(candidateTeamAddress + TeamUniformSlotOffset, TeamUniformSlotCount * 8);
                    for (int candidateSlot = 0; candidateSlot < TeamUniformSlotCount; candidateSlot++)
                        if (checked((long)U64(pointers, candidateSlot * 8)) == uniformAddress) referenceCount++;
                }
                if (referenceCount != 1)
                    throw new InvalidOperationException("The uniform is shared by another slot and cannot be safely removed.");

                byte[] emptySlot = new byte[8];
                try
                {
                    memory.Write(slotAddress, emptySlot);
                    if (!SameBytes(memory.Read(slotAddress, 8), emptySlot))
                        throw new InvalidOperationException("The empty slot did not read back after removal.");
                }
                catch (Exception error)
                {
                    bool restored = false;
                    try
                    {
                        if (SameBytes(memory.Read(slotAddress, 8), emptySlot)) memory.Write(slotAddress, linkedSlot);
                        restored = SameBytes(memory.Read(slotAddress, 8), linkedSlot);
                    }
                    catch (Exception) { restored = false; }
                    throw new InvalidOperationException(error.Message + (restored ? " The uniform link was restored." : " Rollback verification failed."));
                }

                StringBuilder output = new StringBuilder();
                output.Append("{\"schema\":\"nba2k27-team-uniform-unlink-v1\",\"ok\":true");
                output.Append(",\"pid\":").Append(pid).Append(",\"gameCodeHooks\":0,\"physicalWrites\":1");
                output.Append(",\"recordIndex\":").Append(recordIndex).Append(",\"teamId\":").Append(teamId);
                output.Append(",\"slot\":").Append(slot).Append(",\"uniformRecordIndex\":").Append(uniformRecordIndex);
                output.Append(",\"uniformId\":").Append(uniformId).Append(",\"baseFilename\":").Append(Json(expectedBaseFilename));
                output.Append(",\"unlinkReadBackVerified\":true,\"rollbackOnFailure\":true}");
                Console.OutputEncoding = new UTF8Encoding(false);
                Console.WriteLine(output.ToString());
                return 0;
            }
        }

        private static int RunTeamUniformCustomLink(string[] args)
        {
            bool persist = Array.IndexOf(args, "--link-custom-team-uniform") >= 0;
            int pid = ProcessIdFromArgs(args);
            int recordIndex = Int32.Parse(Argument(args, "--record-index"), CultureInfo.InvariantCulture);
            ushort teamId = UInt16.Parse(Argument(args, "--team-id"), CultureInfo.InvariantCulture);
            string expectedTeamName = Argument(args, "--expected-team-name");
            int slot = Int32.Parse(Argument(args, "--slot"), CultureInfo.InvariantCulture);
            int uniformRecordIndex = Int32.Parse(Argument(args, "--uniform-record-index"), CultureInfo.InvariantCulture);
            ushort uniformId = UInt16.Parse(Argument(args, "--uniform-id"), CultureInfo.InvariantCulture);
            string expectedBaseFilename = Argument(args, "--expected-base-filename");
            string newBaseFilename = Argument(args, "--new-base-filename");
            if (slot < 0 || slot >= TeamUniformSlotCount)
                throw new InvalidOperationException("Uniform slot must be between 0 and 31.");
            if (!Regex.IsMatch(expectedBaseFilename, "^u[0-9]{3}[A-Za-z0-9]+_[A-Za-z0-9_]+$")
                || !Regex.IsMatch(newBaseFilename, "^u[0-9]{3}[A-Za-z0-9]+_[A-Za-z0-9_]+$"))
                throw new InvalidOperationException("Uniform base filenames must use the safe u###team_name form.");
            if (expectedBaseFilename.Length != newBaseFilename.Length)
                throw new InvalidOperationException("The first custom-uniform test requires equal-width source and target filenames.");
            byte[] renamedFilename = FixedAscii(newBaseFilename, 0x40, "Custom uniform filename");

            using (Reader memory = new Reader(pid, true))
            {
                long regionBase, regionSize, playerTable;
                int playerCount;
                if (!LocateDatabase(memory, out regionBase, out regionSize, out playerCount, out playerTable))
                    throw new InvalidOperationException("A unique validated NBA 2K27 database was not found.");
                byte[] teamHeader = memory.Read(regionBase + TeamCountOffset, 16);
                int teamCount = checked((int)U64(teamHeader, 0));
                long teamTable = checked((long)U64(teamHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, teamTable, teamCount, TeamStride, 1024)
                    || recordIndex < 0 || recordIndex >= teamCount)
                    throw new InvalidOperationException("The team directory or requested record failed structural validation.");
                long teamAddress = teamTable + (long)recordIndex * TeamStride;
                byte[] teamRecord = memory.Read(teamAddress, TeamStride);
                if (BitConverter.ToUInt16(teamRecord, TeamUniqueIdOffset) != teamId
                    || DecodeUtf16(teamRecord, TeamNameOffset, 48) != expectedTeamName)
                    throw new InvalidOperationException("The selected team changed before the custom jersey test.");
                long slotAddress = teamAddress + TeamUniformSlotOffset + slot * 8L;
                byte[] originalSlot = memory.Read(slotAddress, 8);
                if (U64(originalSlot, 0) != 0)
                    throw new InvalidOperationException("The selected uniform slot is no longer empty.");

                byte[] uniformHeader = memory.Read(regionBase + UniformCountOffset, 16);
                int uniformCount = checked((int)U64(uniformHeader, 0));
                long uniformTable = checked((long)U64(uniformHeader, 8));
                if (!ValidTableRange(regionBase, regionSize, uniformTable, uniformCount, UniformStride, 8192)
                    || uniformRecordIndex < 0 || uniformRecordIndex >= uniformCount)
                    throw new InvalidOperationException("The uniform directory or requested record failed structural validation.");
                long uniformAddress = uniformTable + (long)uniformRecordIndex * UniformStride;
                byte[] uniformRecord = memory.Read(uniformAddress, UniformStride);
                byte[] originalFilename = Slice(uniformRecord, 0x08, 0x40);
                if (BitConverter.ToUInt16(uniformRecord, 0x50) != uniformId
                    || BitConverter.ToUInt16(uniformRecord, 0x52) != teamId
                    || DecodeAscii(uniformRecord, 0x08, 0x40) != expectedBaseFilename)
                    throw new InvalidOperationException("The candidate uniform changed before the custom jersey test.");
                for (int teamIndex = 0; teamIndex < teamCount; teamIndex++)
                {
                    long candidateTeamAddress = teamTable + (long)teamIndex * TeamStride;
                    byte[] pointers = memory.Read(candidateTeamAddress + TeamUniformSlotOffset, TeamUniformSlotCount * 8);
                    for (int candidateSlot = 0; candidateSlot < TeamUniformSlotCount; candidateSlot++)
                        if (checked((long)U64(pointers, candidateSlot * 8)) == uniformAddress)
                            throw new InvalidOperationException("The candidate uniform is already referenced by another team slot.");
                }

                byte[] linkedSlot = BitConverter.GetBytes(uniformAddress);
                bool filenameApplied = false;
                bool slotApplied = false;
                bool filenameRestored = false;
                bool slotRestored = false;
                Exception failure = null;
                try
                {
                    memory.Write(uniformAddress + 0x08, renamedFilename);
                    filenameApplied = true;
                    if (!SameBytes(memory.Read(uniformAddress + 0x08, 0x40), renamedFilename))
                        throw new InvalidOperationException("The custom uniform filename did not read back.");
                    if (persist)
                    {
                        memory.Write(slotAddress, linkedSlot);
                        slotApplied = true;
                        if (!SameBytes(memory.Read(slotAddress, 8), linkedSlot))
                            throw new InvalidOperationException("The custom uniform link did not read back.");
                    }
                    byte[] finalUniform = memory.Read(uniformAddress, UniformStride);
                    if (BitConverter.ToUInt16(finalUniform, 0x50) != uniformId
                        || BitConverter.ToUInt16(finalUniform, 0x52) != teamId
                        || DecodeAscii(finalUniform, 0x08, 0x40) != newBaseFilename)
                        throw new InvalidOperationException("The custom uniform identity failed final verification.");
                }
                catch (Exception error) { failure = error; }
                finally
                {
                    if (slotApplied && failure != null && SameBytes(memory.Read(slotAddress, 8), linkedSlot))
                    {
                        memory.Write(slotAddress, originalSlot);
                        slotRestored = SameBytes(memory.Read(slotAddress, 8), originalSlot);
                    }
                    if (filenameApplied && (failure != null || !persist)
                        && SameBytes(memory.Read(uniformAddress + 0x08, 0x40), renamedFilename))
                    {
                        memory.Write(uniformAddress + 0x08, originalFilename);
                        filenameRestored = SameBytes(memory.Read(uniformAddress + 0x08, 0x40), originalFilename);
                    }
                }
                if (failure != null)
                    throw new InvalidOperationException(failure.Message
                        + ((slotApplied ? slotRestored : true) && filenameRestored ? " The original slot and filename were restored." : " Rollback verification failed."));
                if (!persist && !filenameRestored)
                    throw new InvalidOperationException("The filename round trip passed, but restoration failed.");

                StringBuilder output = new StringBuilder();
                output.Append("{\"schema\":").Append(Json(persist ? "nba2k27-team-uniform-custom-link-v1" : "nba2k27-team-uniform-filename-test-v1")).Append(",\"ok\":true");
                output.Append(",\"pid\":").Append(pid).Append(",\"gameCodeHooks\":0,\"physicalWrites\":").Append(persist ? 2 : 2);
                output.Append(",\"recordIndex\":").Append(recordIndex).Append(",\"teamId\":").Append(teamId);
                output.Append(",\"slot\":").Append(slot).Append(",\"uniformRecordIndex\":").Append(uniformRecordIndex);
                output.Append(",\"uniformId\":").Append(uniformId).Append(",\"oldBaseFilename\":").Append(Json(expectedBaseFilename));
                output.Append(",\"newBaseFilename\":").Append(Json(newBaseFilename));
                output.Append(",\"filenameReadBackVerified\":true,\"linkReadBackVerified\":").Append(persist ? "true" : "false");
                output.Append(",\"restoredOriginalFilename\":").Append(filenameRestored ? "true" : "false").Append(",\"rollbackOnFailure\":true}");
                Console.OutputEncoding = new UTF8Encoding(false);
                Console.WriteLine(output.ToString());
                return 0;
            }
        }

        private static int RunArenaClone(string[] args)
        {
            int pid = ProcessIdFromArgs(args);
            bool populateOnly = Array.IndexOf(args, "--populate-only") >= 0;
            int teamRecordIndex = -1;
            ushort teamId = 0;
            string expectedTeamName = "";
            if (!populateOnly)
            {
                teamRecordIndex = Int32.Parse(Argument(args, "--team-record-index"), CultureInfo.InvariantCulture);
                teamId = UInt16.Parse(Argument(args, "--team-id"), CultureInfo.InvariantCulture);
                expectedTeamName = Argument(args, "--expected-team-name");
            }
            ushort targetId = UInt16.Parse(Argument(args, "--target-id"), CultureInfo.InvariantCulture);
            ushort donorId = UInt16.Parse(Argument(args, "--donor-id"), CultureInfo.InvariantCulture);
            string expectedTargetName = Argument(args, "--expected-target-name");
            string arenaName = Argument(args, "--arena-name");
            string arenaFile = Argument(args, "--arena-file");
            string floorFile = Argument(args, "--floor-file");
            if (targetId == donorId) throw new InvalidOperationException("The custom and donor arena IDs must differ.");

            using (Reader memory = new Reader(pid, true))
            {
                long regionBase, regionSize, playerTable;
                int playerCount;
                if (!LocateDatabase(memory, out regionBase, out regionSize, out playerCount, out playerTable))
                    throw new InvalidOperationException("A unique validated NBA 2K27 database was not found.");
                long arenaTable;
                List<Arena> arenas = ReadArenas(memory, regionBase, regionSize, out arenaTable);
                Arena target = null;
                Arena donor = null;
                int targetMatches = 0;
                int donorMatches = 0;
                foreach (Arena arena in arenas)
                {
                    if (arena.Id == targetId) { target = arena; targetMatches++; }
                    if (arena.Id == donorId) { donor = arena; donorMatches++; }
                }
                if (targetMatches != 1) throw new InvalidOperationException("Custom arena ID " + targetId + " did not resolve to one exact live arena slot.");
                if (donorMatches != 1 || donor == null || donor.EmptyCandidate)
                    throw new InvalidOperationException("Donor arena ID " + donorId + " is missing or empty.");
                if (target.Name != expectedTargetName)
                    throw new InvalidOperationException("The custom arena slot changed before Save; refresh and try again.");
                if (String.IsNullOrEmpty(expectedTargetName) && !target.EmptyCandidate)
                    throw new InvalidOperationException("The selected custom arena slot is not empty.");

                long teamAddress = 0;
                byte[] teamRecord = null;
                if (!populateOnly)
                {
                    byte[] teamHeader = memory.Read(regionBase + TeamCountOffset, 16);
                    int teamCount = checked((int)U64(teamHeader, 0));
                    long teamTable = checked((long)U64(teamHeader, 8));
                    if (!ValidTableRange(regionBase, regionSize, teamTable, teamCount, TeamStride, 1024)
                        || teamRecordIndex < 0 || teamRecordIndex >= teamCount)
                        throw new InvalidOperationException("The team directory or requested record failed structural validation.");
                    teamAddress = teamTable + (long)teamRecordIndex * TeamStride;
                    teamRecord = memory.Read(teamAddress, TeamStride);
                    if (BitConverter.ToUInt16(teamRecord, TeamUniqueIdOffset) != teamId
                        || DecodeUtf16(teamRecord, TeamNameOffset, 48) != expectedTeamName)
                        throw new InvalidOperationException("The selected team changed before Save; refresh and try again.");
                }

                byte[] targetBefore = memory.Read(target.Address, ArenaStride);
                byte[] targetAfter = memory.Read(donor.Address, ArenaStride);
                Buffer.BlockCopy(FixedUtf16(arenaName, 66, "Arena display name"), 0, targetAfter, ArenaNameOffset, 66);
                Buffer.BlockCopy(FixedAscii(arenaFile, 38, "Arena asset stem"), 0, targetAfter, ArenaFileOffset, 38);
                Buffer.BlockCopy(FixedAscii(floorFile, 40, "Floor asset stem"), 0, targetAfter, ArenaFloorOffset, 40);
                Buffer.BlockCopy(BitConverter.GetBytes(targetId), 0, targetAfter, ArenaIdOffset, 2);
                byte[] pointerBefore = populateOnly ? null : Slice(teamRecord, TeamArenaPointerOffset, 8);
                byte[] pointerAfter = populateOnly ? null : BitConverter.GetBytes(target.Address);
                bool targetWritten = false;
                bool pointerWritten = false;
                try
                {
                    memory.Write(target.Address, targetAfter);
                    targetWritten = true;
                    if (!SameBytes(memory.Read(target.Address, ArenaStride), targetAfter))
                        throw new InvalidOperationException("The cloned arena record failed read-back verification.");
                    if (!populateOnly)
                    {
                        memory.Write(teamAddress + TeamArenaPointerOffset, pointerAfter);
                        pointerWritten = true;
                        if (!SameBytes(memory.Read(teamAddress + TeamArenaPointerOffset, 8), pointerAfter))
                            throw new InvalidOperationException("The team-to-arena link failed read-back verification.");
                    }
                }
                catch (Exception error)
                {
                    bool rollbackPassed = true;
                    if (pointerWritten)
                    {
                        try { memory.Write(teamAddress + TeamArenaPointerOffset, pointerBefore); rollbackPassed &= SameBytes(memory.Read(teamAddress + TeamArenaPointerOffset, 8), pointerBefore); }
                        catch (Exception) { rollbackPassed = false; }
                    }
                    if (targetWritten)
                    {
                        try { memory.Write(target.Address, targetBefore); rollbackPassed &= SameBytes(memory.Read(target.Address, ArenaStride), targetBefore); }
                        catch (Exception) { rollbackPassed = false; }
                    }
                    throw new InvalidOperationException(error.Message + (rollbackPassed ? (populateOnly ? " The arena record was rolled back." : " The arena and team link were rolled back.") : " Rollback verification failed."));
                }
                StringBuilder output = new StringBuilder();
                output.Append("{\"schema\":\"nba2k27-arena-clone-v1\",\"ok\":true");
                output.Append(",\"pid\":").Append(pid).Append(",\"gameCodeHooks\":0,\"recordsWritten\":").Append(populateOnly ? 1 : 2);
                output.Append(",\"populateOnly\":").Append(populateOnly ? "true" : "false");
                output.Append(",\"teamId\":").Append(populateOnly ? "null" : teamId.ToString(CultureInfo.InvariantCulture)).Append(",\"targetArenaId\":").Append(targetId).Append(",\"donorArenaId\":").Append(donorId);
                output.Append(",\"targetRecordIndex\":").Append(target.Index).Append(",\"arenaFile\":").Append(Json(arenaFile));
                output.Append(",\"floorFile\":").Append(Json(floorFile)).Append(",\"readBackVerified\":true,\"rollbackOnFailure\":true}");
                Console.OutputEncoding = new UTF8Encoding(false);
                Console.WriteLine(output.ToString());
                return 0;
            }
        }

        private static int RunArenaStatus(string[] args)
        {
            int pid = ProcessIdFromArgs(args);
            using (Reader memory = new Reader(pid))
            {
                long regionBase, regionSize, playerTable;
                int playerCount;
                if (!LocateDatabase(memory, out regionBase, out regionSize, out playerCount, out playerTable))
                    throw new InvalidOperationException("The live arena directory is not ready.");
                long arenaTable;
                List<Arena> arenas = ReadArenas(memory, regionBase, regionSize, out arenaTable);
                if (arenas.Count < 1) throw new InvalidOperationException("The live arena directory failed structural validation.");
                StringBuilder output = new StringBuilder(arenas.Count * 130);
                output.Append("{\"schema\":\"nba2k27-live-arena-status-v1\",\"ok\":true,\"pid\":").Append(pid);
                output.Append(",\"access\":\"read-only\",\"writes\":0,\"arenas\":[");
                for (int index = 0; index < arenas.Count; index++)
                {
                    Arena arena = arenas[index];
                    if (index > 0) output.Append(',');
                    output.Append("{\"recordIndex\":").Append(arena.Index).Append(",\"arenaId\":").Append(arena.Id);
                    output.Append(",\"name\":").Append(Json(arena.Name)).Append(",\"arenaFile\":").Append(Json(arena.ArenaFile));
                    output.Append(",\"floorFile\":").Append(Json(arena.FloorFile)).Append(",\"emptyCandidate\":").Append(arena.EmptyCandidate ? "true" : "false").Append('}');
                }
                output.Append("]}");
                Console.OutputEncoding = new UTF8Encoding(false);
                Console.WriteLine(output.ToString());
                return 0;
            }
        }

        private static int ProcessIdFromArgs(string[] args)
        {
            for (int index = 0; index + 1 < args.Length; index++)
                if (args[index] == "--pid") return Int32.Parse(args[index + 1], CultureInfo.InvariantCulture);
            Process[] matches = Process.GetProcessesByName("NBA2K27");
            if (matches.Length == 0) throw new InvalidOperationException("NBA 2K27 is not running.");
            if (matches.Length != 1) throw new InvalidOperationException("More than one NBA 2K27 process is running.");
            return matches[0].Id;
        }

        private static bool LocateDatabase(Reader memory, out long regionBase, out long regionSize, out int count, out long table)
        {
            regionBase = regionSize = table = 0;
            count = 0;
            int matches = 0;
            foreach (Native.MemoryBasicInformation region in memory.Regions())
            {
                ulong rawSize = region.RegionSize.ToUInt64();
                if (rawSize > Int64.MaxValue) continue;
                long size = (long)rawSize;
                if (region.State != Native.MemCommit || (region.Protect & (Native.PageNoAccess | Native.PageGuard)) != 0
                    || size < MinRegionSize || size > MaxRegionSize) continue;
                long baseAddress = region.BaseAddress.ToInt64();
                try
                {
                    byte[] header = memory.Read(baseAddress + PlayerCountOffset, 16);
                    ulong playerCount = U64(header, 0);
                    long playerTable = checked((long)U64(header, 8));
                    if (playerCount < 100 || playerCount > 8192) continue;
                    if (playerTable != baseAddress + PlayerTableDelta) continue;
                    if (playerTable + checked((long)playerCount * PlayerStride) > baseAddress + size) continue;
                    int valid = 0;
                    for (int sample = 0; sample < 3; sample++)
                    {
                        byte[] record = memory.Read(playerTable + sample * PlayerStride, PlayerNameBytes);
                        string last = DecodeUtf16(record, 0, NamePartBytes);
                        string first = DecodeUtf16(record, NamePartBytes, NamePartBytes);
                        if (ValidName(first) || ValidName(last)) valid++;
                    }
                    if (valid < 3) continue;
                    matches++;
                    regionBase = baseAddress;
                    regionSize = size;
                    count = checked((int)playerCount);
                    table = playerTable;
                }
                catch (Exception) { }
            }
            return matches == 1;
        }

        // Roster table positions vary with the number of preceding records.
        // Validate the entire span inside the discovered allocation, not one roster's delta.
        private static bool ValidTableRange(long regionBase, long regionSize, long pointer, long count, int stride, int maximum)
        {
            if (regionBase < 0x10000 || regionSize < PlayerTableDelta || count < 1 || count > maximum
                || pointer < regionBase || (pointer & 7) != 0) return false;
            long offset = pointer - regionBase;
            long bytes = count * stride;
            return offset >= PlayerTableDelta && offset <= regionSize && bytes <= regionSize - offset;
        }

        private static bool ValidObjectRange(long regionBase, long regionSize, long pointer, int size)
        {
            if (regionBase < 0x10000 || regionSize <= 0 || pointer < regionBase || (pointer & 7) != 0 || size <= 0)
                return false;
            long offset = pointer - regionBase;
            return offset >= 0 && offset <= regionSize && size <= regionSize - offset;
        }

        // Optional custom data can contain non-address sentinels. Validate the
        // unsigned value before converting it, then bound the entire object.
        private static bool TryOptionalObjectAddress(long regionBase, long regionSize, ulong rawPointer, int size, out long pointer)
        {
            pointer = 0;
            if (rawPointer > Int64.MaxValue) return false;
            long candidate = (long)rawPointer;
            if (!ValidObjectRange(regionBase, regionSize, candidate, size)) return false;
            pointer = candidate;
            return true;
        }

        private static List<Arena> ReadArenas(Reader memory, long regionBase, long regionSize, out long arenaTable)
        {
            arenaTable = 0;
            byte[] header = memory.Read(regionBase + ArenaCountOffset, 16);
            ulong countValue = U64(header, 0);
            long pointer = checked((long)U64(header, 8));
            if (countValue > 2048 || !ValidTableRange(regionBase, regionSize, pointer, (long)countValue, ArenaStride, 2048))
                return new List<Arena>();
            arenaTable = pointer;
            List<Arena> arenas = new List<Arena>((int)countValue);
            for (int index = 0; index < (int)countValue; index++)
            {
                long address = pointer + (long)index * ArenaStride;
                byte[] record = memory.Read(address, ArenaStride);
                string name = DecodeUtf16(record, ArenaNameOffset, 66);
                string arenaFile = DecodeAscii(record, ArenaFileOffset, 38);
                string floorFile = DecodeAscii(record, ArenaFloorOffset, 40);
                arenas.Add(new Arena {
                    Index = index,
                    Address = address,
                    Name = name,
                    ArenaFile = arenaFile,
                    FloorFile = floorFile,
                    Id = BitConverter.ToUInt16(record, ArenaIdOffset),
                    EmptyCandidate = String.IsNullOrWhiteSpace(name) && String.IsNullOrWhiteSpace(arenaFile) && String.IsNullOrWhiteSpace(floorFile)
                });
            }
            return arenas;
        }

        private static Dictionary<long, TeamUniform> ReadUniforms(Reader memory, long regionBase, long regionSize)
        {
            var uniforms = new Dictionary<long, TeamUniform>();
            byte[] header = memory.Read(regionBase + UniformCountOffset, 16);
            ulong countValue = U64(header, 0);
            long pointer = checked((long)U64(header, 8));
            if (countValue > 8192 || !ValidTableRange(regionBase, regionSize, pointer, (long)countValue, UniformStride, 8192))
                return uniforms;
            byte[] rows = memory.Read(pointer, checked((int)countValue * UniformStride));
            for (int index = 0; index < (int)countValue; index++)
            {
                long address = pointer + (long)index * UniformStride;
                byte[] record = new byte[UniformStride];
                Buffer.BlockCopy(rows, index * UniformStride, record, 0, UniformStride);
                uint flags160 = BitConverter.ToUInt32(record, 0x160);
                uint flags164 = BitConverter.ToUInt32(record, 0x164);
                uint flags168 = BitConverter.ToUInt32(record, 0x168);
                uint flags16C = BitConverter.ToUInt32(record, 0x16C);
                TeamUniform uniform = new TeamUniform {
                    Index = index,
                    Address = address,
                    Id = BitConverter.ToUInt16(record, 0x50),
                    TeamId = BitConverter.ToUInt16(record, 0x52),
                    BaseFilename = DecodeAscii(record, 0x08, 0x40),
                    SponsorLogoName = DecodeAscii(record, 0x48, 0x08),
                    LocalizedNameToken = BitConverter.ToUInt64(record, 0x58),
                    LocalizedTeamNameToken = BitConverter.ToUInt64(record, 0x60),
                    Name = DecodeAscii(record, 0x68, 0x40),
                    TeamName = DecodeAscii(record, 0xA8, 0x28),
                    UniqueSignatureId = BitConverter.ToUInt32(record, 0xD0),
                    SockClothingItemName = BitConverter.ToUInt32(record, 0xE8),
                    SockClothingItemNameAway = BitConverter.ToUInt32(record, 0xEC),
                    MyTeamTheme = BitConverter.ToUInt32(record, 0xF0),
                    BenchPlayerShirt = BitConverter.ToUInt32(record, 0xF4),
                    SockOption1 = BitConverter.ToUInt32(record, 0xF8),
                    SockOption2 = BitConverter.ToUInt32(record, 0xFC),
                    SockOption3 = BitConverter.ToUInt32(record, 0x100),
                    SockOption4 = BitConverter.ToUInt32(record, 0x104),
                    SockOption5 = BitConverter.ToUInt32(record, 0x108),
                    Type = unchecked((sbyte)record[0x160]),
                    OriginalType = unchecked((sbyte)record[0x161]),
                    LogoType = (int)((flags160 >> 16) & 0xF),
                    LogoBrand = (int)((flags160 >> 20) & 0xF),
                    DominantColor = (int)((flags160 >> 24) & 0x1F),
                    ShouldScorebugUseUniformColor = ((flags160 >> 29) & 1) != 0,
                    UseTeamLogoOnUniform = ((flags164 >> 31) & 1) != 0,
                    CustomCasualShirtColor = (int)(flags164 & 7),
                    CustomJerseyColorRed = (int)((flags164 >> 3) & 7),
                    CustomJerseyColorGreen = (int)((flags164 >> 6) & 7),
                    CustomJerseyColorBlue = (int)((flags164 >> 9) & 7),
                    CustomNameColorRed = (int)((flags164 >> 12) & 7),
                    CustomNameColorGreen = (int)((flags164 >> 15) & 7),
                    CustomNameColorBlue = (int)((flags164 >> 18) & 7),
                    CustomShoeTrimColor1 = (int)((flags164 >> 21) & 7),
                    CustomShoeTrimColor2 = (int)((flags164 >> 24) & 7),
                    CustomShoeDominantColor = (int)((flags164 >> 27) & 7),
                    DisablePhoto = ((flags164 >> 30) & 1) != 0,
                    Side = (int)((flags168 >> 17) & 1),
                    Designation = (int)((flags16C >> 28) & 0xF),
                    IsAlternate = ((flags168 >> 22) & 1) != 0,
                    HomePriority = (int)((flags168 >> 5) & 0xF),
                    AwayPriority = (int)((flags168 >> 9) & 0xF),
                    TopNamePlateStyle = (int)((flags168 >> 13) & 3),
                    BottomNamePlateStyle = (int)((flags168 >> 15) & 3),
                    OverrideNbaPatch = ((flags168 >> 19) & 1) != 0,
                    HasNumberOnShorts = ((flags168 >> 21) & 1) != 0,
                    PrimaryLogoSockColor = ((flags168 >> 23) & 1) != 0,
                    SecondaryLogoSockColor = ((flags168 >> 24) & 1) != 0,
                    TertiaryLogoSockColor = ((flags168 >> 25) & 1) != 0,
                    QuaternaryLogoSockColor = ((flags168 >> 26) & 1) != 0,
                    QuintaryLogoSockColor = ((flags168 >> 27) & 1) != 0,
                    RightAlignFrontNumber = ((flags168 >> 28) & 1) != 0,
                    UniformPrimaryColor = BitConverter.ToUInt32(record, 0x10C),
                    UniformSecondaryColor = BitConverter.ToUInt32(record, 0x110),
                    PrimaryColor = BitConverter.ToUInt32(record, 0x114),
                    SecondaryColor = BitConverter.ToUInt32(record, 0x118),
                    TertiaryColor = BitConverter.ToUInt32(record, 0x11C),
                    QuaternaryColor = BitConverter.ToUInt32(record, 0x120),
                    QuintaryColor = BitConverter.ToUInt32(record, 0x124),
                    SextaryColor = BitConverter.ToUInt32(record, 0x128),
                    PrimaryAccessoryColor = BitConverter.ToUInt32(record, 0x12C),
                    SecondaryAccessoryColor = BitConverter.ToUInt32(record, 0x130),
                    TertiaryAccessoryColor = BitConverter.ToUInt32(record, 0x134),
                    QuaternaryAccessoryColor = BitConverter.ToUInt32(record, 0x138),
                    QuintaryAccessoryColor = BitConverter.ToUInt32(record, 0x13C),
                    UnderShirtColorHome = BitConverter.ToUInt32(record, 0x140),
                    UnderShirtColorAway = BitConverter.ToUInt32(record, 0x144),
                    ShoePrimaryColorAway = BitConverter.ToUInt32(record, 0x148),
                    PrimarySockColor = BitConverter.ToUInt32(record, 0x14C),
                    SecondarySockColor = BitConverter.ToUInt32(record, 0x150),
                    TertiarySockColor = BitConverter.ToUInt32(record, 0x154),
                    QuaternarySockColor = BitConverter.ToUInt32(record, 0x158),
                    QuintarySockColor = BitConverter.ToUInt32(record, 0x15C),
                    HeadbandColorHome = (int)((flags16C >> 12) & 7),
                    HeadbandColorAway = (int)((flags16C >> 15) & 7),
                    HeadbandLogoType = (int)((flags16C >> 18) & 7),
                    LegAccessoryColorHome = (int)(flags16C & 7),
                    LegAccessoryColorAway = (int)((flags16C >> 3) & 7),
                    ArmItemColorHome = (int)((flags16C >> 6) & 7),
                    ArmItemColorAway = (int)((flags16C >> 9) & 7),
                    SockColorHome = (int)((flags16C >> 21) & 7),
                    SockColorAway = (int)((flags16C >> 24) & 7),
                    Flags160 = flags160,
                    Flags164 = flags164,
                    Flags168 = flags168,
                    Flags16C = flags16C,
                };
                long customAddress;
                if (TryOptionalObjectAddress(regionBase, regionSize, U64(record, 0), 0x250, out customAddress))
                {
                    byte[] custom = memory.Read(customAddress, 0x250);
                    uint customFlags = BitConverter.ToUInt32(custom, 0x4C);
                    uniform.HasCustomUniformData = true;
                    uniform.CustomUniformAddress = customAddress;
                    uniform.CustomUniformUniqueId = BitConverter.ToUInt16(custom, 0);
                    uniform.CustomUniformSignature = BitConverter.ToUInt32(custom, 4);
                    uniform.CustomUniformName = DecodeAscii(custom, 0x08, 0x40);
                    uniform.CustomUniformFontNameToken = BitConverter.ToUInt32(custom, 0x48);
                    uniform.CustomUniformNumberOfLayers = (int)(customFlags & 0x3F);
                    uniform.CustomUniformNumberOfOptions = (int)((customFlags >> 6) & 0xF);
                    uniform.CustomUniformInUse = ((customFlags >> 10) & 1) != 0;
                    uniform.CustomUniformPadding = customFlags >> 11;
                    uniform.BakedJerseyFilename = DecodeAscii(custom, 0x50, 0x100);
                    uniform.BakedShortsFilename = DecodeAscii(custom, 0x150, 0x100);
                }
                uniforms[address] = uniform;
            }
            return uniforms;
        }

        private static List<Team> ReadTeams(Reader memory, long regionBase, long regionSize, List<Arena> arenas, long arenaTable,
            Dictionary<long, TeamUniform> uniformDirectory, out long teamTable)
        {
            teamTable = 0;
            byte[] header = memory.Read(regionBase + TeamCountOffset, 16);
            ulong countValue = U64(header, 0);
            long pointer = checked((long)U64(header, 8));
            if (countValue > 1024 || !ValidTableRange(regionBase, regionSize, pointer, (long)countValue, TeamStride, 1024))
                return new List<Team>();
            teamTable = pointer;
            List<Team> teams = new List<Team>();
            for (int index = 0; index < (int)countValue; index++)
            {
                long address = pointer + (long)index * TeamStride;
                byte[] record = memory.Read(address, TeamStride);
                string name = DecodeUtf16(record, TeamNameOffset, 48);
                long arenaAddress = checked((long)U64(record, TeamArenaPointerOffset));
                ushort? arenaId = null;
                string arenaName = "";
                string arenaFile = "";
                if (arenaTable != 0 && arenaAddress >= arenaTable)
                {
                    long difference = arenaAddress - arenaTable;
                    if (difference % ArenaStride == 0 && difference / ArenaStride >= 0 && difference / ArenaStride < arenas.Count)
                    {
                        Arena arena = arenas[(int)(difference / ArenaStride)];
                        arenaId = arena.Id;
                        arenaName = arena.Name;
                        arenaFile = arena.ArenaFile;
                    }
                }
                int rosterType = Bits(record, TeamRosterTypeOffset, 2, 5);
                int historicYear = Bits(record, TeamHistoricYearOffset, 3, 7);
                ushort uniqueId = BitConverter.ToUInt16(record, TeamUniqueIdOffset);
                var uniforms = new List<TeamUniform>();
                var emptyUniformSlots = new List<int>();
                var uniformPointerAddresses = new List<long>();
                int invalidUniformSlotCount = 0;
                for (int slot = 0; slot < TeamUniformSlotCount; slot++)
                {
                    long uniformAddress = checked((long)U64(record, TeamUniformSlotOffset + slot * 8));
                    if (uniformAddress == 0)
                    {
                        emptyUniformSlots.Add(slot);
                        continue;
                    }
                    uniformPointerAddresses.Add(uniformAddress);
                    TeamUniform uniform;
                    if (uniformDirectory.TryGetValue(uniformAddress, out uniform)
                        && uniform.TeamId == uniqueId)
                    {
                        uniforms.Add(uniform.ForSlot(slot));
                    }
                    else invalidUniformSlotCount++;
                }
                teams.Add(new Team {
                    Index = index,
                    Address = address,
                    Name = ValidName(name) ? name : "",
                    CityName = DecodeUtf16(record, TeamCityOffset, 36),
                    CityShort = DecodeUtf16(record, TeamCityShortOffset, 14),
                    StateShort = DecodeUtf16(record, TeamStateShortOffset, 14),
                    MainLogo = DecodeUtf16(record, TeamMainLogoOffset, 72),
                    UniqueId = uniqueId,
                    RosterType = rosterType,
                    RosterTypeName = RosterTypeName(rosterType),
                    HistoricYearId = historicYear,
                    HistoricYear = HistoricYearName(historicYear),
                    ArenaId = arenaId,
                    ArenaName = arenaName,
                    ArenaFile = arenaFile,
                    Uniforms = uniforms,
                    EmptyUniformSlots = emptyUniformSlots,
                    UniformPointerAddresses = uniformPointerAddresses,
                    InvalidUniformSlotCount = invalidUniformSlotCount
                });
            }
            return teams;
        }

        // Optional read-only ratings. Exact serializer fingerprint, stable record,
        // identity and table ownership are all required; no new writes are enabled.
        private static void ReadPlayerRatings(Reader memory, long table, List<Player> players)
        {
            try
            {
                long image = Process.GetProcessById(memory.ProcessId).MainModule.BaseAddress.ToInt64();
                byte[] header = memory.Read(image, 4096);
                int pe = BitConverter.ToInt32(header, 60);
                if (pe < 64 || pe > 3800 || BitConverter.ToUInt32(header, pe + 8) != 0x6AA2013D
                    || BitConverter.ToUInt32(header, pe + 80) != 0x36558000) return;
                using (var hash = System.Security.Cryptography.SHA256.Create())
                {
                    string actual = BitConverter.ToString(hash.ComputeHash(memory.Read(image + RatingSchema.Start, RatingSchema.Length))).Replace("-", "").ToLowerInvariant();
                    if (actual != RatingSchema.Fingerprint) return;
                }
                long root = BitConverter.ToInt64(memory.Read(image + 0x9A47EB8, 8), 0);
                long owner = BitConverter.ToInt64(memory.Read(root + 0xA8, 8), 0);
                if (BitConverter.ToInt64(memory.Read(owner + 0x18, 8), 0) != table) return;
                var results = new List<string>();
                var yearsPro = new List<int?>();
                var bios = new List<PlayerBioData>();
                var archetypes = new List<string>();
                // Read only the cache accepted by the native description getter.
                bool archetypeLayout = SameBytes(memory.Read(image + 0x2DEB753, 7), new byte[]{0xF6,0x81,0x18,0x04,0,0,0x80})
                    && SameBytes(memory.Read(image + 0x2DEB7FF, 7), new byte[]{0x48,0x8B,0x87,0xE8,0,0,0});
                foreach (Player player in players)
                {
                    byte[] record = memory.Read(player.Address, PlayerStride);
                    if (!SameBytes(record, memory.Read(player.Address, PlayerStride))
                        || DecodeUtf16(record, 0, 40) != player.LastName
                        || DecodeUtf16(record, 40, 40) != player.FirstName)
                    { results.Add("null"); yearsPro.Add(null); bios.Add(null); archetypes.Add(null); continue; }
                    var values = new StringBuilder("[");
                    for (int i = 0; i < RatingSchema.Fields.GetLength(0); i++)
                    {
                        if (i > 0) values.Append(',');
                        uint raw = BitConverter.ToUInt32(record, RatingSchema.Fields[i, 0]);
                        values.Append((raw >> RatingSchema.Fields[i, 1]) & (RatingSchema.Fields[i,2]==32?UInt32.MaxValue:((1u << RatingSchema.Fields[i, 2]) - 1)));
                    }
                    results.Add(values.Append(']').ToString());
                    yearsPro.Add((int)((BitConverter.ToUInt32(record,0x3E0)>>17)&31));
                    bios.Add(PlayerBio.Read(record));
                    archetypes.Add(archetypeLayout ? ReadCachedArchetypeKey(record) : null);
                }
                if (BitConverter.ToInt64(memory.Read(owner + 0x18, 8), 0) != table) return;
                for (int i = 0; i < players.Count; i++) { players[i].RatingsJson = results[i]; players[i].YearsPro=yearsPro[i]; players[i].ArchetypeKey=archetypes[i]; }
                for(int i=0;i<players.Count;i++)if(bios[i]!=null){players[i].BioLayoutVerified=true;players[i].BirthYear=bios[i].Year;players[i].BirthMonth=bios[i].Month;players[i].BirthDay=bios[i].Day;players[i].BirthDate=bios[i].DateText();}
            }
            catch { /* Optional data stays unavailable on any validation failure. */ }
        }

        private static string ReadCachedArchetypeKey(byte[] record)
        {
            if (record == null || record.Length != PlayerStride || (record[0x418] & 0x80) == 0) return null;
            ulong key = BitConverter.ToUInt64(record, 0xE8);
            return key == 0 || key == UInt64.MaxValue ? null : key.ToString("x16", CultureInfo.InvariantCulture);
        }

        // Optional, read-only audio enrichment. Never infer offsets on other builds.
        private static void ReadPlayerAudio(Reader memory, long table, List<Player> players)
        {
            try
            {
                long image = Process.GetProcessById(memory.ProcessId).MainModule.BaseAddress.ToInt64();
                byte[] header = memory.Read(image, 4096);
                int pe = BitConverter.ToInt32(header, 60);
                if (pe < 64 || pe > 3800 || BitConverter.ToUInt32(header, pe + 8) != 0x6AA2013D
                    || BitConverter.ToUInt32(header, pe + 80) != 0x36558000) return;
                byte[] anchor = { 0x48,0x8b,0x05,0x46,0x76,0x95,0x07,0x48,0x8b,0x80,0xa8,0,0,0 };
                if (!SameBytes(memory.Read(image + 0x20F086B, anchor.Length), anchor)) return;
                long root = BitConverter.ToInt64(memory.Read(image + 0x9A47EB8, 8), 0);
                long owner = BitConverter.ToInt64(memory.Read(root + 0xA8, 8), 0);
                if (BitConverter.ToInt64(memory.Read(owner + 0x18, 8), 0) != table) return;
                var names = new Dictionary<string, string>[2];
                byte[][] headers = new byte[2][];
                for (int kind = 0; kind < 2; kind++)
                {
                    long at = owner + 0x240 + kind * 16;
                    headers[kind] = memory.Read(at, 16);
                    int n = BitConverter.ToInt32(headers[kind], 0);
                    long data = BitConverter.ToInt64(headers[kind], 8);
                    if (n < 1 || n > 20000 || data < 0x10000) return;
                    byte[] rows = memory.Read(data, checked(n * 48));
                    if (!SameBytes(rows, memory.Read(data, rows.Length))) return;
                    names[kind] = new Dictionary<string, string>(StringComparer.Ordinal);
                    for (int i = 0; i < n; i++)
                    {
                        string name = DecodeUtf16(rows, i * 48, 40);
                        string value = "{\"commentaryId\":" + BitConverter.ToInt16(rows, i * 48 + 40)
                            + ",\"paId\":" + BitConverter.ToInt16(rows, i * 48 + 42) + "}";
                        // Ambiguous shared names must not silently select an arbitrary row.
                        if (names[kind].ContainsKey(name) && names[kind][name] != value) names[kind][name] = "null";
                        else if (!names[kind].ContainsKey(name)) names[kind].Add(name, value);
                    }
                }
                var results = new List<string>();
                foreach (Player player in players)
                {
                    byte[] record = memory.Read(player.Address, PlayerStride);
                    if (DecodeUtf16(record, 0, 40) != player.LastName || DecodeUtf16(record, 40, 40) != player.FirstName
                        || !SameBytes(record, memory.Read(player.Address, PlayerStride))) { results.Add("null"); continue; }
                    string first, last;
                    names[0].TryGetValue(player.FirstName, out first);
                    names[1].TryGetValue(player.LastName, out last);
                    bool overridden = U64(record, 0x50) != 0;
                    results.Add("{\"readOnly\":true,\"fullNameId\":" + BitConverter.ToInt16(record, 0x2E8)
                        + ",\"firstName\":" + (first ?? "null") + ",\"lastName\":" + (overridden ? "null" : last ?? "null")
                        + ",\"nameOverrideActive\":" + (overridden ? "true" : "false") + "}");
                }
                if (BitConverter.ToInt64(memory.Read(root + 0xA8, 8), 0) != owner
                    || BitConverter.ToInt64(memory.Read(owner + 0x18, 8), 0) != table) return;
                for (int kind = 0; kind < 2; kind++)
                    if (!SameBytes(headers[kind], memory.Read(owner + 0x240 + kind * 16, 16))) return;
                for (int i = 0; i < players.Count; i++) players[i].AudioJson = results[i];
            }
            catch (InvalidOperationException) { }
            catch (System.ComponentModel.Win32Exception) { }
            catch (ArgumentException) { }
            catch (OverflowException) { }
        }

        private static List<Player> ReadPlayers(Reader memory, int count, long table, List<Team> teams, long teamTable)
        {
            List<Player> players = new List<Player>(count);
            for (int index = 0; index < count; index++)
            {
                long address = table + (long)index * PlayerStride;
                byte[] record;
                try { record = memory.Read(address, PlayerStride); }
                catch (InvalidOperationException) { continue; }
                string last = DecodeUtf16(record, 0, NamePartBytes);
                string first = DecodeUtf16(record, NamePartBytes, NamePartBytes);
                if (!ValidName(first) && !ValidName(last)) continue;
                long teamAddress = checked((long)U64(record, 0x60));
                int? teamIndex = null;
                string teamName = "";
                if (teamTable != 0 && teamAddress >= teamTable)
                {
                    long difference = teamAddress - teamTable;
                    if (difference % TeamStride == 0)
                    {
                        long candidate = difference / TeamStride;
                        if (candidate >= 0 && candidate < teams.Count)
                        {
                            teamIndex = (int)candidate;
                            teamName = teams[(int)candidate].Name;
                        }
                    }
                }
                ushort? heightRaw = null;
                ushort? wingspanRaw = null;
                long appearanceAddress = checked((long)U64(record, PlayerAppearancePointerOffset));
                if (appearanceAddress != 0)
                {
                    try
                    {
                        byte[] appearance = memory.Read(appearanceAddress, 4);
                        ushort height = BitConverter.ToUInt16(appearance, 0);
                        ushort wingspan = BitConverter.ToUInt16(appearance, 2);
                        if (height >= 12000 && height <= 26000) heightRaw = height;
                        if (wingspan >= 12000 && wingspan <= 32000) wingspanRaw = wingspan;
                    }
                    catch (InvalidOperationException) { }
                }
                float weight = BitConverter.ToSingle(record, PlayerWeightOffset);
                float cachedOverall = BitConverter.ToSingle(record, PlayerCachedOverallOffset);
                int positionId = Bits(record, PlayerPositionOffset, 0, 3);
                int secondaryPositionId = Bits(record, PlayerPositionOffset, 3, 3);
                int jerseyNumberRaw = Bits(record, PlayerJerseyOffset, 3, 7);
                int potentialRaw = Bits(record, PlayerPotentialOffset, 0, 8);
                players.Add(new Player {
                    Index = index,
                    Address = address,
                    FirstName = first,
                    LastName = last,
                    TeamAddress = teamAddress,
                    TeamIndex = teamIndex,
                    TeamName = teamName,
                    CyberfaceId = BitConverter.ToUInt16(record, PlayerCyberfaceIdOffset),
                    PositionId = positionId,
                    SecondaryPositionId = secondaryPositionId,
                    Position = PositionName(positionId),
                    SecondaryPosition = PositionName(secondaryPositionId),
                    JerseyNumberRaw = jerseyNumberRaw,
                    JerseyNumber = jerseyNumberRaw == 100 ? "00" : jerseyNumberRaw.ToString(CultureInfo.InvariantCulture),
                    WeightLbs = !Single.IsNaN(weight) && !Single.IsInfinity(weight) && weight >= 70 && weight <= 500 ? (float?)weight : null,
                    HeightRaw = heightRaw,
                    HeightCm = heightRaw.HasValue ? (double?)heightRaw.Value / 100.0 : null,
                    WingspanRaw = wingspanRaw,
                    WingspanCm = wingspanRaw.HasValue ? (double?)wingspanRaw.Value / 100.0 : null,
                    PotentialRaw = potentialRaw,
                    Potential = PotentialRating(potentialRaw),
                    CachedOverallRaw = !Single.IsNaN(cachedOverall) && !Single.IsInfinity(cachedOverall) && cachedOverall >= 20 && cachedOverall <= 120 ? (float?)cachedOverall : null,
                    Overall = !Single.IsNaN(cachedOverall) && !Single.IsInfinity(cachedOverall) && cachedOverall >= 20 && cachedOverall <= 120 ? (int?)Math.Round(cachedOverall, MidpointRounding.AwayFromZero) : null,
                    // These are the stock PLAYER_APPEARANCE item fields used by
                    // Edit Player and consumed by Blacktop. They are exposed
                    // read-only until each family is verified visually in 2K27.
                    ShoeHome = Bits(record, 0x170, 0, 13),
                    ShoeAway = Bits(record, 0x171, 5, 13),
                    SockHome = Bits(record, 0x160, 4, 4),
                    SockAway = Bits(record, 0x161, 0, 3),
                    Undershirt = Bits(record, 0x2F7, 6, 2),
                    CompressionShorts = Bits(record, 0x193, 6, 3),
                    Headband = Bits(record, 0x1B6, 6, 3),
                    LeftArm = Bits(record, 0x180, 6, 5),
                    RightArm = Bits(record, 0x182, 1, 5),
                    LeftElbow = Bits(record, 0x183, 4, 3),
                    RightElbow = Bits(record, 0x184, 6, 3),
                    LeftWrist = Bits(record, 0x185, 5, 4),
                    RightWrist = Bits(record, 0x276, 0, 4),
                    LeftFingers = Bits(record, 0x34A, 7, 4),
                    RightFingers = Bits(record, 0x34D, 5, 4),
                    LeftLeg = Bits(record, 0x18D, 4, 4),
                    RightLeg = Bits(record, 0x18E, 6, 4),
                    LeftKnee = Bits(record, 0x2F8, 7, 4),
                    RightKnee = Bits(record, 0x1C8, 0, 4),
                    LeftAnkle = Bits(record, 0x191, 4, 3),
                    RightAnkle = Bits(record, 0x192, 5, 3)
                });
            }
            return players;
        }

        private static void WriteReport(int pid, long regionBase, long regionSize, int declaredCount,
            long playerTable, List<Player> players, long teamTable, List<Team> teams,
            Dictionary<long, TeamUniform> uniformDirectory, long arenaTable, List<Arena> arenas, string originsJson)
        {
            int linkedTeams = 0;
            int nonzeroCyberfaceIds = 0;
            int anchorRecordsFound = 0;
            int anchorRecordsMatched = 0;
            int namedTeams = 0;
            int arenaLinkedTeams = 0;
            HashSet<long> ownedUniformAddresses = new HashSet<long>();
            HashSet<long> referencedUniformAddresses = new HashSet<long>();
            int customUniformRecords = 0;
            int unownedBlankFilenameRecords = 0;
            int unownedPopulatedFilenameRecords = 0;
            int unownedStructurallyBlankRecords = 0;
            int unreferencedStructurallyBlankRecords = 0;
            HashSet<string> anchorNamesFound = new HashSet<string>(StringComparer.Ordinal);
            HashSet<string> anchorNamesMatched = new HashSet<string>(StringComparer.Ordinal);
            Dictionary<string, ushort> cyberfaceAnchors = new Dictionary<string, ushort>(StringComparer.Ordinal) {
                { "Joel Embiid", 4090 },
                { "Tyrese Maxey", 8011 },
                { "V.J. Edgecombe", 4120 },
                { "Cooper Flagg", 4117 }
            };
            foreach (Player player in players)
            {
                if (player.TeamIndex.HasValue) linkedTeams++;
                if (player.CyberfaceId != 0) nonzeroCyberfaceIds++;
                string fullName = (player.FirstName + " " + player.LastName).Trim();
                ushort expected;
                if (cyberfaceAnchors.TryGetValue(fullName, out expected))
                {
                    anchorRecordsFound++;
                    anchorNamesFound.Add(fullName);
                    if (player.CyberfaceId == expected)
                    {
                        anchorRecordsMatched++;
                        anchorNamesMatched.Add(fullName);
                    }
                }
            }
            foreach (Team team in teams)
            {
                if (!String.IsNullOrWhiteSpace(team.Name)) namedTeams++;
                if (team.ArenaId.HasValue) arenaLinkedTeams++;
                foreach (TeamUniform uniform in team.Uniforms) ownedUniformAddresses.Add(uniform.Address);
                foreach (long address in team.UniformPointerAddresses)
                    if (uniformDirectory.ContainsKey(address)) referencedUniformAddresses.Add(address);
            }
            foreach (TeamUniform uniform in uniformDirectory.Values)
            {
                if (uniform.HasCustomUniformData) customUniformRecords++;
                if (!ownedUniformAddresses.Contains(uniform.Address))
                {
                    if (String.IsNullOrWhiteSpace(uniform.BaseFilename)) unownedBlankFilenameRecords++;
                    else unownedPopulatedFilenameRecords++;
                    if (uniform.StructurallyBlank()) unownedStructurallyBlankRecords++;
                }
                if (!referencedUniformAddresses.Contains(uniform.Address) && uniform.StructurallyBlank())
                    unreferencedStructurallyBlankRecords++;
            }
            StringBuilder output = new StringBuilder(Math.Max(8192, players.Count * 180));
            output.Append("{\"schema\":\"nba2k27-live-player-research-v1\",\"ok\":true");
            output.Append(",\"pid\":").Append(pid);
            output.Append(",\"origins\":").Append(originsJson);
            output.Append(",\"access\":\"read-only\",\"writes\":0");
            output.Append(",\"regionBase\":").Append(Json("0x" + regionBase.ToString("X")));
            output.Append(",\"regionSize\":").Append(regionSize);
            output.Append(",\"declaredPlayerCount\":").Append(declaredCount);
            output.Append(",\"decodedPlayerCount\":").Append(players.Count);
            output.Append(",\"playerTable\":").Append(Json("0x" + playerTable.ToString("X")));
            output.Append(",\"playerStride\":\"0x4F8\"");
            output.Append(",\"teamTable\":").Append(Json(teamTable == 0 ? "" : "0x" + teamTable.ToString("X")));
            output.Append(",\"teamCount\":").Append(teams.Count);
            output.Append(",\"namedTeamCount\":").Append(namedTeams);
            output.Append(",\"uniformRecordCount\":").Append(uniformDirectory.Count);
            output.Append(",\"ownedUniformRecordCount\":").Append(ownedUniformAddresses.Count);
            output.Append(",\"referencedUniformRecordCount\":").Append(referencedUniformAddresses.Count);
            output.Append(",\"unreferencedUniformRecordCount\":").Append(Math.Max(0, uniformDirectory.Count - referencedUniformAddresses.Count));
            output.Append(",\"unownedUniformRecordCount\":").Append(Math.Max(0, uniformDirectory.Count - ownedUniformAddresses.Count));
            output.Append(",\"unownedUniformBlankFilenameCount\":").Append(unownedBlankFilenameRecords);
            output.Append(",\"unownedUniformPopulatedFilenameCount\":").Append(unownedPopulatedFilenameRecords);
            output.Append(",\"unownedUniformStructurallyBlankCount\":").Append(unownedStructurallyBlankRecords);
            output.Append(",\"unreferencedUniformStructurallyBlankCount\":").Append(unreferencedStructurallyBlankRecords);
            output.Append(",\"customUniformDataRecordCount\":").Append(customUniformRecords);
            output.Append(",\"arenaLinkedTeamCount\":").Append(arenaLinkedTeams);
            output.Append(",\"arenaTable\":").Append(Json(arenaTable == 0 ? "" : "0x" + arenaTable.ToString("X")));
            output.Append(",\"arenaCount\":").Append(arenas.Count);
            output.Append(",\"verifiedTeamLinks\":").Append(linkedTeams);
            output.Append(",\"teamStride\":\"0x1550\",\"teamUniqueIdOffset\":\"0xF20\",\"teamArenaPointerOffset\":\"0xEE0\",\"teamRosterTypeOffset\":\"0x123F bits 2..6\"");
            output.Append(",\"cyberfaceIdOffset\":\"0x110\"");
            output.Append(",\"cyberfaceIdWidthBytes\":2");
            output.Append(",\"nonzeroCyberfaceIds\":").Append(nonzeroCyberfaceIds);
            output.Append(",\"cyberfaceAnchorValidation\":{\"expectedNames\":4,\"namesFound\":").Append(anchorNamesFound.Count);
            output.Append(",\"namesMatched\":").Append(anchorNamesMatched.Count);
            output.Append(",\"recordsFound\":").Append(anchorRecordsFound);
            output.Append(",\"recordsMatched\":").Append(anchorRecordsMatched);
            output.Append(",\"passed\":").Append(anchorNamesFound.Count == 4 && anchorNamesMatched.Count == 4 && anchorRecordsFound == anchorRecordsMatched ? "true" : "false").Append('}');
            output.Append(",\"verifiedFields\":[\"recordIndex\",\"recordAddress\",\"firstName\",\"lastName\",\"cyberfaceId\",\"teamRecordLink\",\"teamName\",\"position\",\"secondaryPosition\",\"jerseyNumber\",\"height\",\"wingspan\",\"weight\",\"potential\"]");
            output.Append(",\"candidateFields\":[\"cachedOverall\",\"playerAppearanceItems\"]");
            output.Append(",\"vitalsOffsets\":{\"appearancePointer\":\"0x78\",\"weight\":\"0x108\",\"jerseyNumber\":\"0x115 bits 3..9\",\"position\":\"0x12D bits 0..2\",\"secondaryPosition\":\"0x12D bits 3..5\",\"cachedOverall\":\"0x1AC\",\"potential\":\"0x45D bits 0..7\"}");
            output.Append(",\"unreferencedPopulatedUniforms\":[");
            bool firstUnreferencedUniform = true;
            foreach (TeamUniform uniform in uniformDirectory.Values)
            {
                if (referencedUniformAddresses.Contains(uniform.Address) || String.IsNullOrWhiteSpace(uniform.BaseFilename)) continue;
                if (!firstUnreferencedUniform) output.Append(',');
                firstUnreferencedUniform = false;
                output.Append("{\"recordIndex\":").Append(uniform.Index);
                output.Append(",\"recordAddress\":").Append(Json("0x" + uniform.Address.ToString("X")));
                output.Append(",\"uniformId\":").Append(uniform.Id);
                output.Append(",\"teamId\":").Append(uniform.TeamId);
                output.Append(",\"baseFilename\":").Append(Json(uniform.BaseFilename));
                output.Append(",\"typeId\":").Append(uniform.Type);
                output.Append(",\"designationId\":").Append(uniform.Designation);
                output.Append(",\"hasCustomUniformData\":").Append(uniform.HasCustomUniformData ? "true" : "false");
                output.Append('}');
            }
            output.Append(']');
            output.Append(",\"players\":[");
            for (int index = 0; index < players.Count; index++)
            {
                Player player = players[index];
                if (index > 0) output.Append(',');
                output.Append("{\"recordIndex\":").Append(player.Index);
                output.Append(",\"recordAddress\":").Append(Json("0x" + player.Address.ToString("X")));
                output.Append(",\"firstName\":").Append(Json(player.FirstName));
                output.Append(",\"lastName\":").Append(Json(player.LastName));
                output.Append(",\"name\":").Append(Json((player.FirstName + " " + player.LastName).Trim()));
                output.Append(",\"cyberfaceId\":").Append(player.CyberfaceId);
                output.Append(",\"teamAddress\":").Append(Json(player.TeamAddress == 0 ? "" : "0x" + player.TeamAddress.ToString("X")));
                output.Append(",\"teamIndex\":").Append(player.TeamIndex.HasValue ? player.TeamIndex.Value.ToString(CultureInfo.InvariantCulture) : "null");
                output.Append(",\"teamName\":").Append(Json(player.TeamName));
                output.Append(",\"positionId\":").Append(player.PositionId);
                output.Append(",\"position\":").Append(Json(player.Position));
                output.Append(",\"secondaryPositionId\":").Append(player.SecondaryPositionId);
                output.Append(",\"secondaryPosition\":").Append(Json(player.SecondaryPosition));
                output.Append(",\"jerseyNumberRaw\":").Append(player.JerseyNumberRaw);
                output.Append(",\"jerseyNumber\":").Append(Json(player.JerseyNumber));
                output.Append(",\"weightLbs\":").Append(player.WeightLbs.HasValue ? player.WeightLbs.Value.ToString("0.##", CultureInfo.InvariantCulture) : "null");
                output.Append(",\"heightRaw\":").Append(player.HeightRaw.HasValue ? player.HeightRaw.Value.ToString(CultureInfo.InvariantCulture) : "null");
                output.Append(",\"heightCm\":").Append(player.HeightCm.HasValue ? player.HeightCm.Value.ToString("0.##", CultureInfo.InvariantCulture) : "null");
                output.Append(",\"wingspanRaw\":").Append(player.WingspanRaw.HasValue ? player.WingspanRaw.Value.ToString(CultureInfo.InvariantCulture) : "null");
                output.Append(",\"wingspanCm\":").Append(player.WingspanCm.HasValue ? player.WingspanCm.Value.ToString("0.##", CultureInfo.InvariantCulture) : "null");
                output.Append(",\"potentialRaw\":").Append(player.PotentialRaw);
                output.Append(",\"potential\":").Append(player.Potential);
                output.Append(",\"audio\":").Append(player.AudioJson);
                output.Append(",\"ratingsRaw\":").Append(player.RatingsJson);
                output.Append(",\"yearsPro\":").Append(player.YearsPro.HasValue?player.YearsPro.Value.ToString(CultureInfo.InvariantCulture):"null");
                output.Append(",\"bioLayoutVerified\":").Append(player.BioLayoutVerified?"true":"false");
                output.Append(",\"birthDate\":").Append(Json(player.BirthDate));
                output.Append(",\"birthYear\":").Append(player.BirthYear.HasValue?player.BirthYear.Value.ToString(CultureInfo.InvariantCulture):"null");
                output.Append(",\"birthMonth\":").Append(player.BirthMonth.HasValue?player.BirthMonth.Value.ToString(CultureInfo.InvariantCulture):"null");
                output.Append(",\"birthDay\":").Append(player.BirthDay.HasValue?player.BirthDay.Value.ToString(CultureInfo.InvariantCulture):"null");
                output.Append(",\"fromIndex\":").Append(player.FromIndex.HasValue?player.FromIndex.Value.ToString(CultureInfo.InvariantCulture):"null");
                output.Append(",\"collegeName\":").Append(player.CollegeName==null?"null":Json(player.CollegeName));
                output.Append(",\"archetypeKey\":").Append(player.ArchetypeKey == null ? "null" : "\"" + player.ArchetypeKey + "\"");
                output.Append(",\"cachedOverallRaw\":").Append(player.CachedOverallRaw.HasValue ? player.CachedOverallRaw.Value.ToString("0.####", CultureInfo.InvariantCulture) : "null");
                output.Append(",\"overall\":").Append(player.Overall.HasValue ? player.Overall.Value.ToString(CultureInfo.InvariantCulture) : "null");
                output.Append(",\"overallStatus\":\"live-cache-candidate\"");
                output.Append(",\"clothing\":{");
                output.Append("\"shoeHome\":").Append(player.ShoeHome);
                output.Append(",\"shoeAway\":").Append(player.ShoeAway);
                output.Append(",\"sockHome\":").Append(player.SockHome);
                output.Append(",\"sockAway\":").Append(player.SockAway);
                output.Append(",\"undershirt\":").Append(player.Undershirt);
                output.Append(",\"compressionShorts\":").Append(player.CompressionShorts);
                output.Append(",\"headband\":").Append(player.Headband);
                output.Append(",\"leftArm\":").Append(player.LeftArm);
                output.Append(",\"rightArm\":").Append(player.RightArm);
                output.Append(",\"leftElbow\":").Append(player.LeftElbow);
                output.Append(",\"rightElbow\":").Append(player.RightElbow);
                output.Append(",\"leftWrist\":").Append(player.LeftWrist);
                output.Append(",\"rightWrist\":").Append(player.RightWrist);
                output.Append(",\"leftFingers\":").Append(player.LeftFingers);
                output.Append(",\"rightFingers\":").Append(player.RightFingers);
                output.Append(",\"leftLeg\":").Append(player.LeftLeg);
                output.Append(",\"rightLeg\":").Append(player.RightLeg);
                output.Append(",\"leftKnee\":").Append(player.LeftKnee);
                output.Append(",\"rightKnee\":").Append(player.RightKnee);
                output.Append(",\"leftAnkle\":").Append(player.LeftAnkle);
                output.Append(",\"rightAnkle\":").Append(player.RightAnkle).Append('}');
                output.Append('}');
            }
            output.Append("],\"teams\":[");
            bool firstTeam = true;
            foreach (Team team in teams)
            {
                if (String.IsNullOrWhiteSpace(team.Name)) continue;
                if (!firstTeam) output.Append(',');
                firstTeam = false;
                output.Append("{\"recordIndex\":").Append(team.Index);
                output.Append(",\"recordAddress\":").Append(Json("0x" + team.Address.ToString("X")));
                output.Append(",\"name\":").Append(Json(team.Name));
                output.Append(",\"cityName\":").Append(Json(team.CityName));
                output.Append(",\"cityShort\":").Append(Json(team.CityShort));
                output.Append(",\"stateShort\":").Append(Json(team.StateShort));
                output.Append(",\"mainLogo\":").Append(Json(team.MainLogo));
                output.Append(",\"teamId\":").Append(team.UniqueId);
                output.Append(",\"rosterTypeId\":").Append(team.RosterType);
                output.Append(",\"rosterType\":").Append(Json(team.RosterTypeName));
                output.Append(",\"historicYearId\":").Append(team.HistoricYearId);
                output.Append(",\"historicYear\":").Append(Json(team.HistoricYear));
                output.Append(",\"arenaId\":").Append(team.ArenaId.HasValue ? team.ArenaId.Value.ToString(CultureInfo.InvariantCulture) : "null");
                output.Append(",\"arenaName\":").Append(Json(team.ArenaName));
                output.Append(",\"arenaFile\":").Append(Json(team.ArenaFile));
                output.Append(",\"uniformSlotCapacity\":").Append(TeamUniformSlotCount);
                output.Append(",\"ownedUniformSlotCount\":").Append(team.Uniforms.Count);
                output.Append(",\"invalidUniformSlotCount\":").Append(team.InvalidUniformSlotCount);
                output.Append(",\"emptyUniformSlots\":[");
                for (int emptyIndex = 0; emptyIndex < team.EmptyUniformSlots.Count; emptyIndex++)
                {
                    if (emptyIndex > 0) output.Append(',');
                    output.Append(team.EmptyUniformSlots[emptyIndex]);
                }
                output.Append(']');
                output.Append(",\"uniforms\":[");
                for (int uniformIndex = 0; uniformIndex < team.Uniforms.Count; uniformIndex++)
                {
                    TeamUniform uniform = team.Uniforms[uniformIndex];
                    if (uniformIndex > 0) output.Append(',');
                    output.Append("{\"slot\":").Append(uniform.Slot);
                    output.Append(",\"recordIndex\":").Append(uniform.Index);
                    output.Append(",\"recordAddress\":").Append(Json("0x" + uniform.Address.ToString("X")));
                    output.Append(",\"uniformId\":").Append(uniform.Id);
                    output.Append(",\"teamId\":").Append(uniform.TeamId);
                    output.Append(",\"baseFilename\":").Append(Json(uniform.BaseFilename));
                    output.Append(",\"sponsorLogoName\":").Append(Json(uniform.SponsorLogoName));
                    output.Append(",\"localizedNameToken\":").Append(Json(uniform.LocalizedNameToken.ToString(CultureInfo.InvariantCulture)));
                    output.Append(",\"localizedTeamNameToken\":").Append(Json(uniform.LocalizedTeamNameToken.ToString(CultureInfo.InvariantCulture)));
                    output.Append(",\"name\":").Append(Json(uniform.Name));
                    output.Append(",\"teamName\":").Append(Json(uniform.TeamName));
                    output.Append(",\"uniqueSignatureId\":").Append(uniform.UniqueSignatureId);
                    output.Append(",\"sockClothingItemNameId\":").Append(uniform.SockClothingItemName);
                    output.Append(",\"sockClothingItemNameAwayId\":").Append(uniform.SockClothingItemNameAway);
                    output.Append(",\"myTeamThemeId\":").Append(uniform.MyTeamTheme);
                    output.Append(",\"benchPlayerShirtId\":").Append(uniform.BenchPlayerShirt);
                    output.Append(",\"sockOptionIds\":[").Append(uniform.SockOption1).Append(',').Append(uniform.SockOption2).Append(',').Append(uniform.SockOption3).Append(',').Append(uniform.SockOption4).Append(',').Append(uniform.SockOption5).Append(']');
                    output.Append(",\"typeId\":").Append(uniform.Type);
                    output.Append(",\"originalTypeId\":").Append(uniform.OriginalType);
                    output.Append(",\"logoTypeId\":").Append(uniform.LogoType);
                    output.Append(",\"logoBrandId\":").Append(uniform.LogoBrand);
                    output.Append(",\"dominantColorId\":").Append(uniform.DominantColor);
                    output.Append(",\"scorebugUsesUniformColor\":").Append(uniform.ShouldScorebugUseUniformColor ? "true" : "false");
                    output.Append(",\"usesTeamLogo\":").Append(uniform.UseTeamLogoOnUniform ? "true" : "false");
                    output.Append(",\"customCasualShirtColor\":").Append(uniform.CustomCasualShirtColor);
                    output.Append(",\"customJerseyColorRed\":").Append(uniform.CustomJerseyColorRed);
                    output.Append(",\"customJerseyColorGreen\":").Append(uniform.CustomJerseyColorGreen);
                    output.Append(",\"customJerseyColorBlue\":").Append(uniform.CustomJerseyColorBlue);
                    output.Append(",\"customNameColorRed\":").Append(uniform.CustomNameColorRed);
                    output.Append(",\"customNameColorGreen\":").Append(uniform.CustomNameColorGreen);
                    output.Append(",\"customNameColorBlue\":").Append(uniform.CustomNameColorBlue);
                    output.Append(",\"customShoeTrimColor1\":").Append(uniform.CustomShoeTrimColor1);
                    output.Append(",\"customShoeTrimColor2\":").Append(uniform.CustomShoeTrimColor2);
                    output.Append(",\"customShoeDominantColor\":").Append(uniform.CustomShoeDominantColor);
                    output.Append(",\"disablePhoto\":").Append(uniform.DisablePhoto ? "true" : "false");
                    output.Append(",\"sideId\":").Append(uniform.Side);
                    output.Append(",\"designationId\":").Append(uniform.Designation);
                    output.Append(",\"isAlternate\":").Append(uniform.IsAlternate ? "true" : "false");
                    output.Append(",\"homePriority\":").Append(uniform.HomePriority);
                    output.Append(",\"awayPriority\":").Append(uniform.AwayPriority);
                    output.Append(",\"topNamePlateStyleId\":").Append(uniform.TopNamePlateStyle);
                    output.Append(",\"bottomNamePlateStyleId\":").Append(uniform.BottomNamePlateStyle);
                    output.Append(",\"overrideNbaPatch\":").Append(uniform.OverrideNbaPatch ? "true" : "false");
                    output.Append(",\"hasNumberOnShorts\":").Append(uniform.HasNumberOnShorts ? "true" : "false");
                    output.Append(",\"primaryLogoSockColor\":").Append(uniform.PrimaryLogoSockColor ? "true" : "false");
                    output.Append(",\"secondaryLogoSockColor\":").Append(uniform.SecondaryLogoSockColor ? "true" : "false");
                    output.Append(",\"tertiaryLogoSockColor\":").Append(uniform.TertiaryLogoSockColor ? "true" : "false");
                    output.Append(",\"quaternaryLogoSockColor\":").Append(uniform.QuaternaryLogoSockColor ? "true" : "false");
                    output.Append(",\"quintaryLogoSockColor\":").Append(uniform.QuintaryLogoSockColor ? "true" : "false");
                    output.Append(",\"rightAlignFrontNumber\":").Append(uniform.RightAlignFrontNumber ? "true" : "false");
                    output.Append(",\"uniformPrimaryColor\":").Append(uniform.UniformPrimaryColor);
                    output.Append(",\"uniformSecondaryColor\":").Append(uniform.UniformSecondaryColor);
                    output.Append(",\"primaryColor\":").Append(uniform.PrimaryColor);
                    output.Append(",\"secondaryColor\":").Append(uniform.SecondaryColor);
                    output.Append(",\"tertiaryColor\":").Append(uniform.TertiaryColor);
                    output.Append(",\"quaternaryColor\":").Append(uniform.QuaternaryColor);
                    output.Append(",\"quintaryColor\":").Append(uniform.QuintaryColor);
                    output.Append(",\"sextaryColor\":").Append(uniform.SextaryColor);
                    output.Append(",\"primaryAccessoryColor\":").Append(uniform.PrimaryAccessoryColor);
                    output.Append(",\"secondaryAccessoryColor\":").Append(uniform.SecondaryAccessoryColor);
                    output.Append(",\"tertiaryAccessoryColor\":").Append(uniform.TertiaryAccessoryColor);
                    output.Append(",\"quaternaryAccessoryColor\":").Append(uniform.QuaternaryAccessoryColor);
                    output.Append(",\"quintaryAccessoryColor\":").Append(uniform.QuintaryAccessoryColor);
                    output.Append(",\"underShirtColorHome\":").Append(uniform.UnderShirtColorHome);
                    output.Append(",\"underShirtColorAway\":").Append(uniform.UnderShirtColorAway);
                    output.Append(",\"shoePrimaryColorAway\":").Append(uniform.ShoePrimaryColorAway);
                    output.Append(",\"primarySockColor\":").Append(uniform.PrimarySockColor);
                    output.Append(",\"secondarySockColor\":").Append(uniform.SecondarySockColor);
                    output.Append(",\"tertiarySockColor\":").Append(uniform.TertiarySockColor);
                    output.Append(",\"quaternarySockColor\":").Append(uniform.QuaternarySockColor);
                    output.Append(",\"quintarySockColor\":").Append(uniform.QuintarySockColor);
                    output.Append(",\"headbandColorHome\":").Append(uniform.HeadbandColorHome);
                    output.Append(",\"headbandColorAway\":").Append(uniform.HeadbandColorAway);
                    output.Append(",\"headbandLogoTypeId\":").Append(uniform.HeadbandLogoType);
                    output.Append(",\"legAccessoryColorHome\":").Append(uniform.LegAccessoryColorHome);
                    output.Append(",\"legAccessoryColorAway\":").Append(uniform.LegAccessoryColorAway);
                    output.Append(",\"armItemColorHome\":").Append(uniform.ArmItemColorHome);
                    output.Append(",\"armItemColorAway\":").Append(uniform.ArmItemColorAway);
                    output.Append(",\"sockColorHome\":").Append(uniform.SockColorHome);
                    output.Append(",\"sockColorAway\":").Append(uniform.SockColorAway);
                    output.Append(",\"flags160\":").Append(uniform.Flags160);
                    output.Append(",\"flags164\":").Append(uniform.Flags164);
                    output.Append(",\"flags168\":").Append(uniform.Flags168);
                    output.Append(",\"flags16C\":").Append(uniform.Flags16C);
                    output.Append(",\"hasCustomUniformData\":").Append(uniform.HasCustomUniformData ? "true" : "false");
                    if (uniform.HasCustomUniformData)
                    {
                        output.Append(",\"customUniformAddress\":").Append(Json("0x" + uniform.CustomUniformAddress.ToString("X")));
                        output.Append(",\"customUniformUniqueId\":").Append(uniform.CustomUniformUniqueId);
                        output.Append(",\"customUniformSignature\":").Append(uniform.CustomUniformSignature);
                        output.Append(",\"customUniformName\":").Append(Json(uniform.CustomUniformName));
                        output.Append(",\"customUniformFontNameToken\":").Append(uniform.CustomUniformFontNameToken);
                        output.Append(",\"customUniformNumberOfLayers\":").Append(uniform.CustomUniformNumberOfLayers);
                        output.Append(",\"customUniformNumberOfOptions\":").Append(uniform.CustomUniformNumberOfOptions);
                        output.Append(",\"customUniformInUse\":").Append(uniform.CustomUniformInUse ? "true" : "false");
                        output.Append(",\"customUniformPadding\":").Append(uniform.CustomUniformPadding);
                        output.Append(",\"bakedJerseyFilename\":").Append(Json(uniform.BakedJerseyFilename));
                        output.Append(",\"bakedShortsFilename\":").Append(Json(uniform.BakedShortsFilename));
                    }
                    output.Append('}');
                }
                output.Append(']');
                output.Append('}');
            }
            output.Append("],\"arenas\":[");
            for (int index = 0; index < arenas.Count; index++)
            {
                Arena arena = arenas[index];
                if (index > 0) output.Append(',');
                output.Append("{\"recordIndex\":").Append(arena.Index);
                output.Append(",\"recordAddress\":").Append(Json("0x" + arena.Address.ToString("X")));
                output.Append(",\"arenaId\":").Append(arena.Id);
                output.Append(",\"name\":").Append(Json(arena.Name));
                output.Append(",\"arenaFile\":").Append(Json(arena.ArenaFile));
                output.Append(",\"floorFile\":").Append(Json(arena.FloorFile));
                output.Append(",\"emptyCandidate\":").Append(arena.EmptyCandidate ? "true" : "false");
                output.Append('}');
            }
            output.Append("]}");
            Console.OutputEncoding = new UTF8Encoding(false);
            Console.WriteLine(output.ToString());
        }

        private static int Main(string[] args)
        {
            try
            {
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--team-staff" || args[index] == "--team-staff-write" || args[index] == "--team-rotations" || args[index] == "--team-rotations-write") return RunTeamStaff(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--arena-status") return RunArenaStatus(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--clone-arena") return RunArenaClone(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--test-team-uniform-link") return RunTeamUniformLinkTest(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--link-team-uniform") return RunTeamUniformLinkTest(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--unlink-team-uniform") return RunTeamUniformUnlink(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--test-team-uniform-filename") return RunTeamUniformCustomLink(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--link-custom-team-uniform") return RunTeamUniformCustomLink(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--write-team-uniform") return RunTeamUniformEdit(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--write-team") return RunTeamEdit(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--season-import") return RunSeasonImportCommit(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--season-import-snapshot") return RunSeasonImportSnapshot(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--season-stats") return RunSeasonStats(args);
                for (int index = 0; index < args.Length; index++)
                    if (args[index] == "--write-player") return RunPlayerEdit(args);
                int pid = ProcessIdFromArgs(args);
                using (Reader memory = new Reader(pid))
                {
                    long regionBase, regionSize, playerTable;
                    int count;
                    if (!LocateDatabase(memory, out regionBase, out regionSize, out count, out playerTable))
                        throw new InvalidOperationException("A unique validated NBA 2K27 player database was not found. Wait at the full main menu and refresh again.");
                    long arenaTable;
                    List<Arena> arenas = ReadArenas(memory, regionBase, regionSize, out arenaTable);
                    if (arenas.Count < 1) throw new InvalidOperationException("The arena directory failed structural validation.");
                    long teamTable;
                    Dictionary<long, TeamUniform> uniforms = ReadUniforms(memory, regionBase, regionSize);
                    List<Team> teams = ReadTeams(memory, regionBase, regionSize, arenas, arenaTable, uniforms, out teamTable);
                    List<Player> players = ReadPlayers(memory, count, playerTable, teams, teamTable);
                    ReadPlayerAudio(memory, playerTable, players);
                    ReadPlayerRatings(memory, playerTable, players);
                    if (players.Count < 100)
                        throw new InvalidOperationException("The player-name validation threshold was not met.");
                    string originsJson=ReadPlayerOrigins(memory,playerTable,players);
                    WriteReport(pid, regionBase, regionSize, count, playerTable, players, teamTable, teams, uniforms, arenaTable, arenas,originsJson);
                }
                return 0;
            }
            catch (Exception error)
            {
                Console.Error.WriteLine(error.Message);
                return 1;
            }
        }
    }
}
