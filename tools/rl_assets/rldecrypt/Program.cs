// Decrypts Rocket League packages so UModel can read them.
// usage: rldecrypt <keys.txt> <outDir> <package.upk>...
using RlUpk.Core.RocketLeague;
using RlUpk.Core.RocketLeague.Decryption;
using RlUpk.Core.Serialization.Default;

var provider = new DecryptionProvider(args[0]);
Directory.CreateDirectory(args[1]);
int failed = 0;
foreach (var file in args.Skip(2))
{
    var outPath = Path.Combine(args[1], Path.GetFileName(file));
    using (var input = File.Open(file, FileMode.Open, FileAccess.Read, FileShare.Read))
    using (var output = File.Create(outPath))
    {
        var unpacker = new RLPackageUnpacker(input, provider, FileSummarySerializer.GetDefaultSerializer());
        unpacker.Unpack(output);
        if (unpacker.Valid) { Console.WriteLine($"ok   {Path.GetFileName(file)}"); continue; }
    }
    File.Delete(outPath);
    Console.WriteLine($"FAIL {Path.GetFileName(file)}");
    failed++;
}
return failed == 0 ? 0 : 1;
