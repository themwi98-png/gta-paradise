// cwconv: thin CLI over CodeWalker.Core for the gta-paradise pipeline.
//
//   cwconv convert <in.xml | dir> <out-dir>
//       CodeWalker XML (.ybn.xml, .ydr.xml, .ytyp.xml, .ymap.xml, ...) -> binary.
//       Directories are converted recursively, mirroring the folder layout.
//   cwconv sample <gta-dir> <rpf-relpath> <ext> <out-dir> [max] [name-filter]
//       Exports files of one type from a game archive to CodeWalker XML, as
//       reference templates. Read-only; never writes to the game folder.
//   cwconv extract <gta-dir> <archive-path> <out-file>
//       Copies one file out of a game archive (read-only).
//   cwconv pack <dlc-name> <out-dir> [--gta <gta-dir>] [--disable-list <file>] <dir | rpfname=dir[:cache][:mapdata][:mp][:meta]>...
//       (--gta: NG encryption, loads without OpenIV)
//   cwconv dlclist-add <gta-dir> <dlc-name> [archive] [--open]
//   cwconv rpf-replace <gta-dir> <archive> <nested-archive-path> <dir> [--open]
//       Replaces a nested archive with one holding every file in <dir>.
//   cwconv rpf-extract <gta-dir> <archive> <regex> <out-dir>
//       Copies matching files out as stored.
//   cwconv toxml <file> <out-dir>
//       Converts a loose .ynv/.ynd/.ybn/.ymap back to XML.
//   cwconv rpf-empty <gta-dir> <archive> <regex> [--open]
//       Replaces nested archives whose path matches with empty ones.
//   cwconv rpf-put <gta-dir> <archive> <path-in-archive> <file> [--open]
//       Adds/replaces a file inside an archive (e.g. water.xml in the mods update.rpf).
//       Adds the pack to dlclist.xml in update.rpf (or a given copy).
//       Builds <out-dir>/<dlc-name>/dlc.rpf (content.xml, setup2.xml, level rpf).
//   cwconv extents <gta-dir> <out.csv>
//       Bounding boxes of all base-game .ybn files (read-only).
//   cwconv list <file.rpf> [gta-dir]
//       Lists an archive (and nested archives), verifying .ybn files load.
//   cwconv ybninfo <file.ybn>
//       Summarises a binary bounds file.
//   cwconv api [filter]
//       Lists CodeWalker.Core types/methods (development aid).

using System.Reflection;
using System.Xml;
using CodeWalker.GameFiles;
using SharpDX;

if (args.Length == 0)
{
    Console.Error.WriteLine("usage: cwconv convert <in> <out-dir> | sample <gta-dir> <rpf> <ext> <out-dir> [max] [filter] | api [filter]");
    return 1;
}

try
{
    return args[0] switch
    {
        "convert" => Convert(args[1], args[2]),
        "sample" => Sample(args[1], args[2], args[3], args[4], args.Length > 5 ? int.Parse(args[5]) : 5, args.Length > 6 ? args[6] : ""),
        "extract" => Extract(args[1], args[2], args[3]),
        "ybninfo" => YbnInfo(args[1]),
        "list" => List(args[1], args.Length > 2 ? args[2] : null),
        "extents" => Extents(args[1], args[2]),
        "pack" => Pack(args[1], args[2], args.Skip(3).ToArray()),
        "toxml" => ToXml(args[1], args[2]),
        "rpf-extract" => RpfExtract(args[1], args[2], args[3], args[4], args.Contains("--keep-paths")),
        "rpf-replace" => RpfReplace(args[1], args[2], args[3], args[4], args.Contains("--open")),
        "rpf-empty" => RpfEmpty(args[1], args[2], args[3], args.Contains("--open")),
        "rpf-put" => RpfPut(args[1], args[2], args[3], args[4], args.Contains("--open")),
        "archetypes" => Archetypes(args[1], args[2], args[3], args[4]),
        "data-pack" => DataPack(args[1], args[2], int.Parse(args[3]), args.Skip(4).ToArray()),
        "dlclist-add" => DlcListAdd(args[1], args[2], args.Skip(3).FirstOrDefault(a => a != "--open"), args.Contains("--open")),
        "dlclist-remove" => DlcListAdd(args[1], args[2], args.Skip(3).FirstOrDefault(a => a != "--open"), args.Contains("--open"), remove: true),
        "api" => Api(args.Length > 1 ? args[1] : "Xml"),
        _ => throw new ArgumentException($"unknown command {args[0]}"),
    };
}
catch (Exception e)
{
    Console.Error.WriteLine($"error: {e}");
    return 1;
}

static int Convert(string input, string outDir)
{
    var files = Directory.Exists(input)
        ? Directory.GetFiles(input, "*.xml", SearchOption.AllDirectories)
        : new[] { input };
    var root = Directory.Exists(input) ? Path.GetFullPath(input) : Path.GetDirectoryName(Path.GetFullPath(input));
    int ok = 0, failed = 0;
    foreach (var f in files)
    {
        var name = Path.GetFileName(f);
        var format = XmlMeta.GetXMLFormat(name.ToLowerInvariant(), out int trim);
        if (format == MetaFormat.XML)
        {
            Console.Error.WriteLine($"skip {f}: unrecognised XML type");
            continue;
        }
        var outName = name.Substring(0, name.Length - trim);
        var rel = Path.GetRelativePath(root, Path.GetDirectoryName(Path.GetFullPath(f)));
        var dest = Path.Combine(outDir, rel, outName);
        try
        {
            var doc = new XmlDocument();
            doc.Load(f);
            // fpathin is the folder holding referenced assets (e.g. .dds textures for .ydr).
            var data = XmlMeta.GetData(doc, format, Path.GetDirectoryName(Path.GetFullPath(f)));
            if (data == null) throw new Exception("CodeWalker returned no data");
            Directory.CreateDirectory(Path.GetDirectoryName(dest));
            File.WriteAllBytes(dest, data);
            ok++;
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"FAILED {f}: {e.Message}");
            failed++;
        }
    }
    Console.WriteLine($"converted {ok} file(s), {failed} failed");
    return failed == 0 ? 0 : 2;
}

static int Sample(string gtaDir, string rpfRel, string ext, string outDir, int max, string filter)
{
    Console.WriteLine("loading keys from game executable...");
    GTA5Keys.LoadFromPath(gtaDir, null);
    var rpf = new RpfFile(Path.Combine(gtaDir, rpfRel), rpfRel);
    rpf.ScanStructure(null, e => Console.Error.WriteLine(e));
    Directory.CreateDirectory(outDir);
    int n = 0;
    foreach (var entry in AllEntries(rpf).OfType<RpfFileEntry>())
    {
        if (!entry.Name.EndsWith("." + ext, StringComparison.OrdinalIgnoreCase)) continue;
        if (filter.Length > 0 && !entry.Path.Contains(filter, StringComparison.OrdinalIgnoreCase)) continue;
        var data = entry.File.ExtractFile(entry);
        string xml, fname;
        switch (ext.ToLowerInvariant())
        {
            case "ybn": xml = MetaXml.GetXml(RpfFile.GetFile<YbnFile>(entry, data), out fname); break;
            case "ytyp": xml = MetaXml.GetXml(RpfFile.GetFile<YtypFile>(entry, data), out fname); break;
            case "ymap": xml = MetaXml.GetXml(RpfFile.GetFile<YmapFile>(entry, data), out fname); break;
            case "ydr": xml = MetaXml.GetXml(RpfFile.GetFile<YdrFile>(entry, data), out fname, outDir); break;
            case "ytd": xml = YtdXml.GetXml(RpfFile.GetFile<YtdFile>(entry, data), outDir); fname = entry.Name + ".xml"; break;
            case "ynd": xml = MetaXml.GetXml(RpfFile.GetFile<YndFile>(entry, data), out fname); break;
            case "ymt": xml = MetaXml.GetXml(RpfFile.GetFile<YmtFile>(entry, data), out fname); break;
            case "ynv": xml = MetaXml.GetXml(RpfFile.GetFile<YnvFile>(entry, data), out fname); break;
            default: throw new ArgumentException($"unsupported sample type {ext}");
        }
        File.WriteAllText(Path.Combine(outDir, fname), xml);
        Console.WriteLine($"{entry.Path} -> {fname} ({data.Length} bytes)");
        if (++n >= max) break;
    }
    return 0;
}

// Copies one file out of a game archive, e.g.
// extract <gta-dir> common.rpf\data\materials\materials.dat out.dat
static int Extract(string gtaDir, string path, string outFile)
{
    GTA5Keys.LoadFromPath(gtaDir, null);
    var rpfRel = path.Substring(0, path.IndexOf(".rpf", StringComparison.OrdinalIgnoreCase) + 4);
    var rpf = new RpfFile(Path.Combine(gtaDir, rpfRel), rpfRel);
    rpf.ScanStructure(null, e => Console.Error.WriteLine(e));
    var entry = AllEntries(rpf).OfType<RpfFileEntry>().FirstOrDefault(e => e.Path.Equals(path, StringComparison.OrdinalIgnoreCase))
        ?? throw new FileNotFoundException(path);
    File.WriteAllBytes(outFile, entry.File.ExtractFile(entry));
    Console.WriteLine($"{entry.Path} -> {outFile}");
    return 0;
}

// Builds a DLC pack: <out-dir>/<name>/dlc.rpf containing content.xml,
// setup2.xml and x64/levels/<name>/<name>.rpf holding every file from the
// given folders (.ybn, .ydr, .ytd, .ytyp, .ymap ...). OPEN encryption, as
// used by mods. Never writes into the game folder unless pointed there.
static int Pack(string name, string outDir, string[] inputs)
{
    name = name.ToLowerInvariant();
    // "--gta <dir>" loads keys and NG-encrypts the archives so the game reads
    // them without OpenIV.asi; otherwise OPEN (mod-loader) encryption is used.
    var enc = RpfEncryption.OPEN;
    var specs = inputs.ToList();
    // "--disable-list <file>": base-game archives (one platform:/ path per
    // line) to disable in story mode, e.g. to remove Los Santos.
    var disable = new List<string>();
    int di = specs.IndexOf("--disable-list");
    if (di >= 0)
    {
        disable = File.ReadAllLines(specs[di + 1]).Select(l => l.Trim()).Where(l => l.Length > 0 && !l.StartsWith("#")).Distinct().ToList();
        specs.RemoveRange(di, 2);
    }
    int gi = specs.IndexOf("--gta");
    if (gi >= 0)
    {
        GTA5Keys.LoadFromPath(specs[gi + 1], null);
        enc = RpfEncryption.NG;
        specs.RemoveRange(gi, 2);
    }
    // Each spec is "dir" (-> <name>.rpf) or "rpfname=dir[:opt...]", one level
    // archive per spec. The default is the route verified in game (build
    // 3889): a GROUP_MAP_SP change set with a plain filesToEnable and no
    // cache file, applied when paradise.asi calls ON_ENTER_SP. Options for
    // experiments: cache (write a cache_y.dat), mapdata (mapChangeSetData
    // with associatedMap instead of filesToEnable; never mounted in testing),
    // mp (Online GROUP_MAP / MO_JIM_L11; needs ON_ENTER_MP). "meta" marks a
    // map metadata archive (.ytyp/.ymap): CONTENTS_DLC_MAP_DATA, loaded
    // completely, with each .ytyp registered as a DLC_ITYP_REQUEST.
    var levelsSpec = specs.Select(sp =>
    {
        var eq = sp.IndexOf('=');
        var rpfName = eq > 0 ? sp[..eq].ToLowerInvariant() : name;
        var parts = (eq > 0 ? sp[(eq + 1)..] : sp).Split(':');
        // Keep a drive letter ("C:/...") attached to the path.
        var dir = parts[0].Length == 1 && parts.Length > 1 ? parts[0] + ":" + parts[1] : parts[0];
        var opts = parts.Skip(parts[0].Length == 1 && parts.Length > 1 ? 2 : 1).ToHashSet();
        return (rpfName, dir, cache: opts.Contains("cache"), plain: !opts.Contains("mapdata"), mp: opts.Contains("mp"), meta: opts.Contains("meta"));
    }).ToList();

    var packDir = Path.Combine(outDir, name);
    Directory.CreateDirectory(packDir);
    var dlcPath = Path.Combine(packDir, "dlc.rpf");
    if (File.Exists(dlcPath)) File.Delete(dlcPath);

    // Map content goes in map change set groups: GROUP_MAP_SP (story mode)
    // or GROUP_MAP (Online). Neither is applied by default in story mode: a
    // script must call ON_ENTER_SP / ON_ENTER_MP (paradise.asi calls
    // ON_ENTER_SP). Static collision enabled from GROUP_STARTUP is mounted
    // but never streamed on current builds. The optional cache file (named
    // after the change set hash) mirrors Rockstar's map DLCs; it turned out
    // not to be needed.
    var dlc = RpfFile.CreateNew(packDir, "dlc.rpf", enc);
    var x64 = RpfFile.CreateDirectory(dlc.Root, "x64");
    var levelsDir = RpfFile.CreateDirectory(RpfFile.CreateDirectory(RpfFile.CreateDirectory(x64, "levels"), "gta5"), name);
    RpfDirectoryEntry cacheDir = null;
    var changeSets = new List<LevelSet>();
    foreach (var (rpfName, dir, cache, plain, mp, meta) in levelsSpec)
    {
        var levelRel = $"%PLATFORM%/levels/gta5/{name}/{rpfName}.rpf";
        var changeSet = $"{name}_{rpfName}_MAP".ToUpperInvariant().Replace($"{name.ToUpperInvariant()}_{name.ToUpperInvariant()}_", $"{name.ToUpperInvariant()}_");
        var ityps = new List<string>();
        changeSets.Add(new LevelSet(changeSet, levelRel, cache, plain, mp, meta, ityps));
        var inner = RpfFile.CreateNew(levelsDir, rpfName + ".rpf", enc);
        var bounds = new List<(uint hash, Vector3 min, Vector3 max)>();
        int n = 0;
        // Intermediate files (CodeWalker XML, manifests, loose textures) stay out.
        var skip = new[] { ".xml", ".json", ".dds", ".png", ".bin" };
        foreach (var f in Directory.GetFiles(dir).Where(f => !skip.Contains(Path.GetExtension(f).ToLowerInvariant())).OrderBy(f => f))
        {
            var data = File.ReadAllBytes(f);
            RpfFile.CreateFile(inner.Root, Path.GetFileName(f), data, true);
            if (meta && f.EndsWith(".ytyp", StringComparison.OrdinalIgnoreCase)) ityps.Add(Path.GetFileName(f));
            if (f.EndsWith(".ybn", StringComparison.OrdinalIgnoreCase))
            {
                var ybn = new YbnFile();
                ybn.Load(data);
                bounds.Add((Joaat(Path.GetFileNameWithoutExtension(f)), ybn.Bounds.BoxMin, ybn.Bounds.BoxMax));
            }
            n++;
        }
        var note = "no cache";
        if (cache)
        {
            cacheDir ??= RpfFile.CreateDirectory(RpfFile.CreateDirectory(x64, "data"), "cacheloaderdata_dlc");
            var levelHash = Joaat($"dlc_{name}:/x64/levels/gta5/{name}/{rpfName}.rpf");
            var cacheName = $"{name}_{Joaat(changeSet)}_cache_y.dat";
            RpfFile.CreateFile(cacheDir, cacheName, CacheDat(new[] { levelHash }, bounds), true);
            note = $"cache {cacheName} with {bounds.Count} bounds";
        }
        Console.WriteLine($"  {rpfName}.rpf: {n} file(s), change set {changeSet} ({(mp ? "GROUP_MAP/L11" : "GROUP_MAP_SP/L00")}{(plain ? ", plain" : "")}{(meta ? $", map data, {ityps.Count} ytyp" : "")}), {note}");
    }
    var removeSet = disable.Count > 0 ? $"{name.ToUpperInvariant()}_REMOVE_BASE_MAP" : null;
    RpfFile.CreateFile(dlc.Root, "content.xml", System.Text.Encoding.UTF8.GetBytes(ContentXml(name, changeSets, removeSet, disable)), true);
    var spSets = changeSets.Where(c => !c.Mp).Select(c => c.ChangeSet).ToList();
    if (removeSet != null) spSets.Insert(0, removeSet);
    RpfFile.CreateFile(dlc.Root, "setup2.xml", System.Text.Encoding.UTF8.GetBytes(Setup2Xml(name,
        spSets, changeSets.Where(c => c.Mp).Select(c => c.ChangeSet))), true);
    if (removeSet != null) Console.WriteLine($"  {removeSet}: disables {disable.Count} base-game archive(s)");
    Console.WriteLine($"packed {levelsSpec.Count} level archive(s) into {dlcPath} ({enc})");
    return 0;
}

// Lists the archetypes (name, bounding box, LOD distance) defined by every
// .ytyp whose path matches <regex> in the archives named by <rpf-glob>
// (relative to the game folder), as CSV, e.g.
//   archetypes <gta> "x64*.rpf" "levels/gta5/props/" out/props/gta_archetypes.csv
static int Archetypes(string gtaDir, string rpfGlob, string pattern, string outCsv)
{
    GTA5Keys.LoadFromPath(gtaDir, null);
    var re = new System.Text.RegularExpressions.Regex(pattern, System.Text.RegularExpressions.RegexOptions.IgnoreCase);
    var dir = Path.GetDirectoryName(Path.Combine(gtaDir, rpfGlob))!;
    var files = Directory.GetFiles(dir, Path.GetFileName(rpfGlob));
    var sb = new System.Text.StringBuilder("ytyp,name,minx,miny,minz,maxx,maxy,maxz,lod\n");
    int n = 0;
    // Archetype names are only stored as hashes; models are named after them.
    var rpfs = files.OrderBy(f => f).Select(f => { var r = new RpfFile(f, Path.GetRelativePath(gtaDir, f)); r.ScanStructure(null, e => { }); return r; }).ToList();
    var names = new Dictionary<uint, string>();
    foreach (var rpf in rpfs)
        foreach (var e in AllEntries(rpf).OfType<RpfFileEntry>())
        {
            var ext = Path.GetExtension(e.Name).ToLowerInvariant();
            if (ext == ".ydr" || ext == ".yft" || ext == ".ydd") names[Joaat(Path.GetFileNameWithoutExtension(e.Name))] = Path.GetFileNameWithoutExtension(e.Name).ToLowerInvariant();
        }
    foreach (var rpf in rpfs)
    {
        foreach (var e in AllEntries(rpf).OfType<RpfFileEntry>().Where(e => e.Name.EndsWith(".ytyp", StringComparison.OrdinalIgnoreCase) && re.IsMatch(e.Path.Replace('\\', '/'))))
        {
            try
            {
                var ytyp = RpfFile.GetFile<YtypFile>(e, e.File.ExtractFile(e));
                foreach (var a in ytyp.AllArchetypes ?? Array.Empty<Archetype>())
                {
                    var b = a._BaseArchetypeDef;
                    var nm = names.TryGetValue(b.name.Hash, out var known) ? known : a.Name;
                    sb.Append($"{e.Name},{nm},{b.bbMin.X},{b.bbMin.Y},{b.bbMin.Z},{b.bbMax.X},{b.bbMax.Y},{b.bbMax.Z},{b.lodDist}\n");
                    n++;
                }
            }
            catch (Exception ex) { Console.Error.WriteLine($"{e.Path}: {ex.Message}"); }
        }
    }
    File.WriteAllText(outCsv, sb.ToString());
    Console.WriteLine($"{n} archetypes from {files.Length} archive(s) -> {outCsv}");
    return 0;
}

// Packs loose data files (no map content) into a DLC that loads after
// Rockstar's packs, e.g.
//   data-pack bpcrash out/dlc 101 HANDLING_FILE=out/crash/handling.meta
// Each file goes in common/data/ and is enabled at startup, so entries in it
// (e.g. vehicle handling) replace the same-named ones loaded before.
static int DataPack(string name, string outDir, int order, string[] specs)
{
    name = name.ToLowerInvariant();
    var packDir = Path.Combine(outDir, name);
    Directory.CreateDirectory(packDir);
    var dlcPath = Path.Combine(packDir, "dlc.rpf");
    if (File.Exists(dlcPath)) File.Delete(dlcPath);
    var dlc = RpfFile.CreateNew(packDir, "dlc.rpf", RpfEncryption.OPEN);
    var data = RpfFile.CreateDirectory(RpfFile.CreateDirectory(dlc.Root, "common"), "data");
    var files = new List<(string type, string rel)>();
    foreach (var spec in specs)
    {
        int eq = spec.IndexOf('=');
        var (type, file) = (spec[..eq], spec[(eq + 1)..]);
        RpfFile.CreateFile(data, Path.GetFileName(file).ToLowerInvariant(), File.ReadAllBytes(file), true);
        files.Add((type, $"common/data/{Path.GetFileName(file).ToLowerInvariant()}"));
    }
    var set = name.ToUpperInvariant() + "_AUTOGEN";
    var sb = new System.Text.StringBuilder();
    void L(string line) => sb.Append(line).Append('\n');
    L("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    L("<CDataFileMgr__ContentsOfDataFileXml>");
    L(" <disabledFiles />");
    L(" <includedXmlFiles />");
    L(" <includedDataFiles />");
    L(" <dataFiles>");
    foreach (var (type, rel) in files)
    {
        L("  <Item>");
        L($"   <filename>dlc_{name}:/{rel}</filename>");
        L($"   <fileType>{type}</fileType>");
        L("   <overlay value=\"false\" />");
        L("   <disabled value=\"true\" />");
        L("   <persistent value=\"false\" />");
        L("  </Item>");
    }
    L(" </dataFiles>");
    L(" <contentChangeSets>");
    L("  <Item>");
    L($"   <changeSetName>{set}</changeSetName>");
    L("   <mapChangeSetData />");
    L("   <filesToInvalidate />");
    L("   <filesToDisable />");
    L("   <filesToEnable>");
    foreach (var (_, rel) in files) L($"    <Item>dlc_{name}:/{rel}</Item>");
    L("   </filesToEnable>");
    L("   <txdToLoad />");
    L("   <txdToUnload />");
    L("   <residentResources />");
    L("   <unregisterResources />");
    L("   <requiresLoadingScreen value=\"false\" />");
    L("  </Item>");
    L(" </contentChangeSets>");
    L(" <patchFiles />");
    L("</CDataFileMgr__ContentsOfDataFileXml>");
    RpfFile.CreateFile(dlc.Root, "content.xml", System.Text.Encoding.UTF8.GetBytes(sb.ToString()), true);
    var setup = Setup2Xml(name, Array.Empty<string>(), Array.Empty<string>())
        .Replace("<order value=\"100\" />", $"<order value=\"{order}\" />");
    RpfFile.CreateFile(dlc.Root, "setup2.xml", System.Text.Encoding.UTF8.GetBytes(setup), true);
    Console.WriteLine($"packed {files.Count} data file(s) into {dlcPath} (order {order})");
    return 0;
}

// Rockstar's case-insensitive one-at-a-time hash.
static uint Joaat(string s)
{
    uint h = 0;
    foreach (var c in s.ToLowerInvariant())
    {
        h += (byte)c;
        h += h << 10;
        h ^= h >> 6;
    }
    h += h << 3;
    h ^= h >> 11;
    h += h << 15;
    return h;
}

// Writes a cache_y.dat (version 46), format reverse-engineered from the
// official DLC caches: 100-byte "[VERSION]\n46\n" header, a text fileDates
// list, then <module> blocks of "<name>\n" + u32 byte count + fixed records.
// BoundsStore records: u32 name hash, min xyz, max xyz, u32 0x7F00.
static byte[] CacheDat(uint[] archiveHashes, List<(uint hash, Vector3 min, Vector3 max)> bounds)
{
    var ms = new MemoryStream();
    var w = new BinaryWriter(ms);
    void Text(string s) => w.Write(System.Text.Encoding.ASCII.GetBytes(s));
    var header = new byte[100];
    System.Text.Encoding.ASCII.GetBytes("[VERSION]\n46\n").CopyTo(header, 0);
    w.Write(header);
    Text("<fileDates>\n");
    var now = DateTime.UtcNow.ToFileTimeUtc();
    foreach (var h in archiveHashes) Text($"{h} {now}\n");
    Text("</fileDates>\n");
    Text("<module>\nfwMapDataStore\n"); w.Write(0u); Text("</module>\n");
    Text("<module>\nCInteriorProxy\n"); w.Write(0u); Text("</module>\n");
    Text("<module>\nBoundsStore\n");
    w.Write((uint)(bounds.Count * 32));
    foreach (var (hash, min, max) in bounds)
    {
        w.Write(hash);
        w.Write(min.X); w.Write(min.Y); w.Write(min.Z);
        w.Write(max.X); w.Write(max.Y); w.Write(max.Z);
        w.Write(0x7F00u);
    }
    Text("</module>\n");
    return ms.ToArray();
}

// Replaces every nested archive whose path matches <regex> with an empty one,
// e.g. rpf-empty <gta> mods/update/x64/dlcpacks/patchday3ng/dlc.rpf "levels/gta5/_hills/" --open
// Used on mods-folder copies of patch packs whose Los Santos sectors the game
// mounts even when no data file lists them.
static int RpfEmpty(string gtaDir, string rpfPath, string pattern, bool open)
{
    GTA5Keys.LoadFromPath(gtaDir, null);
    var rpf = new RpfFile(Path.GetFullPath(rpfPath), Path.GetFileName(rpfPath));
    rpf.ScanStructure(null, e => Console.Error.WriteLine(e));
    var re = new System.Text.RegularExpressions.Regex(pattern, System.Text.RegularExpressions.RegexOptions.IgnoreCase);
    var targets = rpf.Children
        .Where(c => c.ParentFileEntry != null && re.IsMatch(c.ParentFileEntry.Path.Replace('\\', '/')) && c.AllEntries.OfType<RpfFileEntry>().Any())
        .ToList();
    if (targets.Count == 0) { Console.WriteLine($"{rpfPath}: nothing to empty"); return 0; }
    if (rpf.Encryption == RpfEncryption.NG)
    {
        if (!open) throw new InvalidOperationException("archive is NG-encrypted; pass --open to convert it (only on a mods-folder copy)");
        RpfFile.SetEncryptionType(rpf, RpfEncryption.OPEN);
    }
    foreach (var child in targets)
    {
        var entry = child.ParentFileEntry;
        var dir = entry.Parent;
        RpfFile.DeleteEntry(entry);
        RpfFile.CreateNew(dir, entry.Name, RpfEncryption.OPEN);
    }
    Console.WriteLine($"{rpfPath}: emptied {targets.Count} archive(s)");
    return 0;
}

// Converts a loose resource file (.ynv, .ynd, .ybn, .ymap ...) back to
// CodeWalker XML, e.g. to check what a conversion produced.
static int ToXml(string file, string outDir)
{
    var data = File.ReadAllBytes(file);
    var name = Path.GetFileName(file);
    var entry = RpfFile.CreateResourceFileEntry(ref data, 0);
    data = ResourceBuilder.Decompress(data);
    entry.Name = name;
    entry.NameLower = name.ToLowerInvariant();
    string xml, fname;
    switch (Path.GetExtension(name).ToLowerInvariant())
    {
        case ".ynv": xml = MetaXml.GetXml(RpfFile.GetFile<YnvFile>(entry, data), out fname); break;
        case ".ynd": xml = MetaXml.GetXml(RpfFile.GetFile<YndFile>(entry, data), out fname); break;
        case ".ybn": xml = MetaXml.GetXml(RpfFile.GetFile<YbnFile>(entry, data), out fname); break;
        case ".ymap": xml = MetaXml.GetXml(RpfFile.GetFile<YmapFile>(entry, data), out fname); break;
        default: throw new ArgumentException($"unsupported type {name}");
    }
    Directory.CreateDirectory(outDir);
    File.WriteAllText(Path.Combine(outDir, fname), xml);
    Console.WriteLine($"{file} -> {Path.Combine(outDir, fname)}");
    return 0;
}

// Replaces a nested archive with a new one holding every file in <dir>,
// e.g. rpf-replace <gta> mods/update/update.rpf x64/levels/gta5/paths.rpf out/gta_paths --open
static int RpfReplace(string gtaDir, string rpfPath, string inner, string dir, bool open)
{
    GTA5Keys.LoadFromPath(gtaDir, null);
    var rpf = new RpfFile(Path.GetFullPath(rpfPath), Path.GetFileName(rpfPath));
    rpf.ScanStructure(null, e => Console.Error.WriteLine(e));
    var want = inner.Replace('\\', '/').Trim('/').ToLowerInvariant();
    var child = rpf.Children.FirstOrDefault(c => c.ParentFileEntry != null
        && c.ParentFileEntry.Path.Replace('\\', '/').ToLowerInvariant().EndsWith("/" + want));
    if (rpf.Encryption == RpfEncryption.NG)
    {
        if (!open) throw new InvalidOperationException("archive is NG-encrypted; pass --open to convert it (only on a mods-folder copy)");
        RpfFile.SetEncryptionType(rpf, RpfEncryption.OPEN);
    }
    RpfDirectoryEntry parent;
    string name;
    if (child != null)
    {
        parent = child.ParentFileEntry.Parent;
        name = child.ParentFileEntry.Name;
        RpfFile.DeleteEntry(child.ParentFileEntry);
    }
    else
    {
        // Not there yet: create it, and its folders.
        var parts = want.Split('/', StringSplitOptions.RemoveEmptyEntries);
        parent = rpf.Root;
        foreach (var part in parts.Take(parts.Length - 1))
            parent = parent.Directories.FirstOrDefault(d => d.Name.Equals(part, StringComparison.OrdinalIgnoreCase)) ?? RpfFile.CreateDirectory(parent, part);
        name = parts[^1];
    }
    var fresh = RpfFile.CreateNew(parent, name, RpfEncryption.OPEN);
    var files = Directory.Exists(dir) ? Directory.GetFiles(dir).OrderBy(f => f, StringComparer.OrdinalIgnoreCase).ToList() : new List<string>();
    foreach (var f in files) RpfFile.CreateFile(fresh.Root, Path.GetFileName(f), File.ReadAllBytes(f), true);
    Console.WriteLine($"{(child != null ? "replaced" : "created")} {rpfPath}:{inner} with {files.Count} file(s) from {dir}");
    return 0;
}

// Copies every file (in nested archives too) whose path matches <regex> out
// of a game archive as stored (resources keep their RSC7 header).
static int RpfExtract(string gtaDir, string rpfRel, string pattern, string outDir, bool keepPaths = false)
{
    GTA5Keys.LoadFromPath(gtaDir, null);
    var rpf = new RpfFile(Path.Combine(gtaDir, rpfRel), rpfRel);
    rpf.ScanStructure(null, e => Console.Error.WriteLine(e));
    var re = new System.Text.RegularExpressions.Regex(pattern, System.Text.RegularExpressions.RegexOptions.IgnoreCase);
    Directory.CreateDirectory(outDir);
    int n = 0;
    foreach (var e in AllEntries(rpf).OfType<RpfFileEntry>().Where(e => re.IsMatch(e.Path.Replace('\\', '/'))))
    {
        var data = e.File.ExtractFile(e);
        // Resources come back decompressed without their header; store them
        // as a game file again so they can be packed elsewhere.
        if (e is RpfResourceFileEntry res) data = ResourceBuilder.AddResourceHeader(res, ResourceBuilder.Compress(data));
        var dst = keepPaths ? Path.Combine(outDir, e.Path.Replace('\\', '/')) : Path.Combine(outDir, e.Name);
        Directory.CreateDirectory(Path.GetDirectoryName(dst)!);
        File.WriteAllBytes(dst, data);
        n++;
    }
    Console.WriteLine($"extracted {n} file(s) from {rpfRel} -> {outDir}");
    return 0;
}

// Adds or replaces one file inside an archive, creating folders as needed,
// e.g. rpf-put <gta> mods/update/update.rpf common/data/levels/gta5/water.xml water.xml --open
// (NG archives are switched to OPEN first, as CodeWalker cannot write NG).
static int RpfPut(string gtaDir, string rpfPath, string inner, string file, bool open)
{
    GTA5Keys.LoadFromPath(gtaDir, null);
    var rpf = new RpfFile(Path.GetFullPath(rpfPath), @"update\update.rpf");
    rpf.ScanStructure(null, e => Console.Error.WriteLine(e));
    if (rpf.Encryption == RpfEncryption.NG)
    {
        if (!open) throw new InvalidOperationException("archive is NG-encrypted; pass --open to convert it (only on a mods-folder copy)");
        RpfFile.SetEncryptionType(rpf, RpfEncryption.OPEN);
    }
    var parts = inner.Replace('\\', '/').Split('/', StringSplitOptions.RemoveEmptyEntries);
    var dir = rpf.Root;
    foreach (var part in parts.Take(parts.Length - 1))
    {
        var next = dir.Directories.FirstOrDefault(d => d.Name.Equals(part, StringComparison.OrdinalIgnoreCase));
        dir = next ?? RpfFile.CreateDirectory(dir, part);
    }
    RpfFile.CreateFile(dir, parts[^1], File.ReadAllBytes(file), true);
    Console.WriteLine($"put {file} -> {rpfPath}:{inner}");
    return 0;
}

// Adds "dlcpacks:/<name>/" to common\data\dlclist.xml inside update.rpf (or the
// given archive, e.g. a test copy), keeping the archive's encryption.
static int DlcListAdd(string gtaDir, string name, string rpfPath, bool open, bool remove = false)
{
    GTA5Keys.LoadFromPath(gtaDir, null);
    rpfPath ??= Path.Combine(gtaDir, "update", "update.rpf");
    // The relative path must be the real "update\update.rpf": NG keys are
    // derived from the archive's file name and size.
    var rpf = new RpfFile(Path.GetFullPath(rpfPath), @"update\update.rpf");
    rpf.ScanStructure(null, e => Console.Error.WriteLine(e));
    // CodeWalker cannot write NG headers, so the archive (normally the copy in
    // the OpenIV mods folder) is switched to OPEN first. Individually
    // NG-encrypted entries stay as they are; OpenIV.asi decrypts them.
    if (rpf.Encryption == RpfEncryption.NG)
    {
        if (!open) throw new InvalidOperationException("archive is NG-encrypted; pass --open to convert it (only on a mods-folder copy)");
        RpfFile.SetEncryptionType(rpf, RpfEncryption.OPEN);
        Console.WriteLine("converted archive header to OPEN");
    }
    var entry = rpf.AllEntries.OfType<RpfFileEntry>().FirstOrDefault(e => e.Path.EndsWith(@"common\data\dlclist.xml", StringComparison.OrdinalIgnoreCase))
        ?? throw new FileNotFoundException("dlclist.xml not found in " + rpfPath);
    var xml = System.Text.Encoding.UTF8.GetString(rpf.ExtractFile(entry));
    var item = $"<Item>dlcpacks:/{name.ToLowerInvariant()}/</Item>";
    if (remove)
    {
        if (!xml.Contains(item)) { Console.WriteLine($"not listed: {item}"); return 0; }
        xml = System.Text.RegularExpressions.Regex.Replace(xml, @"[ \t]*" + System.Text.RegularExpressions.Regex.Escape(item) + @"\r?\n?", "");
        RpfFile.CreateFile(entry.Parent, entry.Name, System.Text.Encoding.UTF8.GetBytes(xml), true);
        Console.WriteLine($"removed {item} from {rpfPath}");
        return 0;
    }
    if (xml.Contains(item))
    {
        Console.WriteLine($"already listed: {item}");
        return 0;
    }
    int at = xml.LastIndexOf("</Paths>", StringComparison.Ordinal);
    if (at < 0) throw new InvalidDataException("no </Paths> in dlclist.xml");
    xml = xml.Insert(at, "\t" + item + "\r\n\t");
    RpfFile.CreateFile(entry.Parent, entry.Name, System.Text.Encoding.UTF8.GetBytes(xml), true);
    Console.WriteLine($"added {item} to {rpfPath} ({rpf.Encryption})");
    return 0;
}

static string ContentXml(string name, List<LevelSet> sets, string removeSet = null, List<string> disable = null)
{
    var sb = new System.Text.StringBuilder();
    void L(string line) => sb.Append(line).Append('\n');
    string Enable(string rel) => $"<Item>dlc_{name}:/{rel}</Item>";

    L("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    L("<CDataFileMgr__ContentsOfDataFileXml>");
    L(" <disabledFiles />");
    L(" <includedXmlFiles />");
    L(" <includedDataFiles />");
    L(" <dataFiles>");
    foreach (var s in sets)
    {
        L("  <Item>");
        L($"   <filename>dlc_{name}:/{s.LevelRel}</filename>");
        L("   <fileType>RPF_FILE</fileType>");
        if (s.Meta) L("   <loadCompletely value=\"true\" />");
        L("   <overlay value=\"false\" />");
        L("   <disabled value=\"true\" />");
        L("   <persistent value=\"true\" />");
        if (s.Meta) L("   <contents>CONTENTS_DLC_MAP_DATA</contents>");
        L("  </Item>");
        foreach (var ityp in s.Ityps)
        {
            L("  <Item>");
            L($"   <filename>dlc_{name}:/{s.LevelRel}/{ityp}</filename>");
            L("   <fileType>DLC_ITYP_REQUEST</fileType>");
            L("   <overlay value=\"false\" />");
            L("   <disabled value=\"true\" />");
            L("   <persistent value=\"true\" />");
            L("  </Item>");
        }
    }
    L(" </dataFiles>");
    L(" <contentChangeSets>");
    // Empty startup change set (packs conventionally have one).
    L("  <Item>");
    L($"   <changeSetName>{name.ToUpperInvariant()}_AUTOGEN</changeSetName>");
    L("   <mapChangeSetData />");
    L("   <filesToInvalidate />");
    L("   <filesToDisable />");
    L("   <filesToEnable />");
    L("   <txdToLoad />");
    L("   <txdToUnload />");
    L("   <residentResources />");
    L("   <unregisterResources />");
    L("   <requiresLoadingScreen value=\"false\" />");
    L("  </Item>");
    foreach (var s in sets)
    {
        L("  <Item>");
        L($"   <changeSetName>{s.ChangeSet}</changeSetName>");
        if (s.Plain)
        {
            L("   <mapChangeSetData />");
        }
        else
        {
            L("   <mapChangeSetData>");
            L("    <Item>");
            L($"     <associatedMap>{(s.Mp ? "MO_JIM_L11" : "MO_JIM_L00")}</associatedMap>");
            L("     <filesToInvalidate />");
            L("     <filesToDisable />");
            L("     <filesToEnable>");
            L($"      {Enable(s.LevelRel)}");
            L("     </filesToEnable>");
            L("    </Item>");
            L("   </mapChangeSetData>");
        }
        L("   <filesToInvalidate />");
        L("   <filesToDisable />");
        if (s.Plain)
        {
            L("   <filesToEnable>");
            L($"    {Enable(s.LevelRel)}");
            foreach (var ityp in s.Ityps) L($"    {Enable(s.LevelRel + "/" + ityp)}");
            L("   </filesToEnable>");
        }
        else
        {
            L("   <filesToEnable />");
        }
        L("   <txdToLoad />");
        L("   <txdToUnload />");
        L("   <residentResources />");
        L("   <unregisterResources />");
        L("   <requiresLoadingScreen value=\"false\" />");
        L($"   <useCacheLoader value=\"{(s.Cache ? "true" : "false")}\" />");
        L("  </Item>");
    }
    if (removeSet != null)
    {
        // Removes base-map archives (e.g. Los Santos) when story-mode map
        // content is applied; a loading screen covers the swap, as for
        // Rockstar's own map changes.
        L("  <Item>");
        L($"   <changeSetName>{removeSet}</changeSetName>");
        L("   <mapChangeSetData />");
        // Rockstar replaces base-map areas by invalidating the archives;
        // filesToDisable alone had no effect on base archives in testing.
        L("   <filesToInvalidate>");
        foreach (var f in disable) L($"    <Item>{f}</Item>");
        L("   </filesToInvalidate>");
        L("   <filesToDisable>");
        foreach (var f in disable) L($"    <Item>{f}</Item>");
        L("   </filesToDisable>");
        L("   <filesToEnable />");
        L("   <txdToLoad />");
        L("   <txdToUnload />");
        L("   <residentResources />");
        L("   <unregisterResources />");
        L("   <requiresLoadingScreen value=\"true\" />");
        L("   <loadingScreenContext>LOADINGSCREEN_CONTEXT_LAST_FRAME</loadingScreenContext>");
        L("  </Item>");
    }
    L(" </contentChangeSets>");
    L(" <patchFiles />");
    L("</CDataFileMgr__ContentsOfDataFileXml>");
    return sb.ToString();
}

static string Setup2Xml(string name, IEnumerable<string> spSets, IEnumerable<string> mpSets)
{
    var sb = new System.Text.StringBuilder();
    void L(string line) => sb.Append(line).Append('\n');
    void Group(string group, IEnumerable<string> sets)
    {
        if (!sets.Any()) return;
        L("  <Item>");
        L($"   <NameHash>{group}</NameHash>");
        L("   <ContentChangeSets>");
        foreach (var c in sets) L($"    <Item>{c}</Item>");
        L("   </ContentChangeSets>");
        L("  </Item>");
    }

    L("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    L("<SSetupData>");
    L($" <deviceName>dlc_{name}</deviceName>");
    L(" <datFile>content.xml</datFile>");
    L(" <timeStamp>10/10/2026 00:00:00</timeStamp>");
    L($" <nameHash>{name}</nameHash>");
    L(" <contentChangeSets />");
    L(" <contentChangeSetGroups>");
    Group("GROUP_STARTUP", new[] { name.ToUpperInvariant() + "_AUTOGEN" });
    Group("GROUP_MAP_SP", spSets);
    Group("GROUP_MAP", mpSets);
    L(" </contentChangeSetGroups>");
    L(" <startupScript />");
    L(" <scriptCallstackSize value=\"0\" />");
    L(" <type>EXTRACONTENT_COMPAT_PACK</type>");
    L(" <order value=\"100\" />");
    L(" <minorOrder value=\"0\" />");
    L(" <isLevelPack value=\"false\" />");
    L(" <dependencyPackHash />");
    L(" <requiredVersion />");
    L(" <subPackCount value=\"0\" />");
    L("</SSetupData>");
    return sb.ToString();
}

// Lists an archive's files (including nested archives) with sizes, and
// checks that resource files inside it load.
static int List(string rpfPath, string gtaDir)
{
    // NG-encrypted archives need the game's keys (pass the game folder).
    if (gtaDir != null) GTA5Keys.LoadFromPath(gtaDir, null);
    var rpf = new RpfFile(Path.GetFullPath(rpfPath), Path.GetFileName(rpfPath));
    rpf.ScanStructure(null, e => Console.Error.WriteLine(e));
    int files = 0, bad = 0;
    foreach (var e in AllEntries(rpf).OfType<RpfFileEntry>())
    {
        files++;
        if (e.Name.EndsWith(".ybn"))
        {
            try { RpfFile.GetFile<YbnFile>(e, e.File.ExtractFile(e)); }
            catch (Exception ex) { bad++; Console.Error.WriteLine($"BAD {e.Path}: {ex.Message}"); }
        }
        if (files <= 8) Console.WriteLine($"{e.Path} ({e.GetFileSize()} bytes)");
    }
    Console.WriteLine($"{files} files, {bad} unreadable");
    return bad == 0 ? 0 : 2;
}

// Writes a CSV (path,minx,miny,minz,maxx,maxy,maxz) of every static
// collision file in the base game's x64*.rpf archives (read-only), used to
// find empty space for placing new map content.
static int Extents(string gtaDir, string outCsv)
{
    GTA5Keys.LoadFromPath(gtaDir, null);
    using var w = new StreamWriter(outCsv);
    w.WriteLine("path,minx,miny,minz,maxx,maxy,maxz");
    int n = 0, bad = 0;
    foreach (var rpfPath in Directory.GetFiles(gtaDir, "x64*.rpf").OrderBy(p => p))
    {
        var rel = Path.GetFileName(rpfPath);
        var rpf = new RpfFile(rpfPath, rel);
        rpf.ScanStructure(null, null);
        foreach (var e in AllEntries(rpf).OfType<RpfFileEntry>().Where(e => e.Name.EndsWith(".ybn")))
        {
            try
            {
                var b = RpfFile.GetFile<YbnFile>(e, e.File.ExtractFile(e)).Bounds;
                if (b == null) continue;
                w.WriteLine(FormattableString.Invariant($"{e.Path},{b.BoxMin.X},{b.BoxMin.Y},{b.BoxMin.Z},{b.BoxMax.X},{b.BoxMax.Y},{b.BoxMax.Z}"));
                n++;
            }
            catch { bad++; }
        }
        Console.WriteLine($"{rel}: {n} bounds so far");
    }
    Console.WriteLine($"{n} bounds written, {bad} failed");
    return 0;
}

// Summarises a binary .ybn: per child bounds type, counts, BVH presence.
static int YbnInfo(string path)
{
    var ybn = new YbnFile();
    ybn.Load(File.ReadAllBytes(path));
    void Dump(Bounds b, string indent)
    {
        var line = $"{indent}{b.GetType().Name} box {b.BoxMin} .. {b.BoxMax}";
        if (b is BoundGeometry g)
            line += $" verts {g.VerticesCount} polys {g.PolygonsCount} mats {g.MaterialsCount} quantum {g.Quantum}";
        if (b is BoundBVH bvh)
            line += $" bvh nodes {bvh.BVH?.Nodes?.data_items?.Length ?? -1} trees {bvh.BVH?.Trees?.data_items?.Length ?? -1}";
        Console.WriteLine(line);
        if (b is BoundComposite c && c.Children?.data_items != null)
            foreach (var ch in c.Children.data_items) if (ch != null) Dump(ch, indent + "  ");
    }
    Dump(ybn.Bounds, "");
    return 0;
}

// Walks an archive including nested archives (e.g. dlc .rpf inside .rpf).
static IEnumerable<RpfEntry> AllEntries(RpfFile rpf)
{
    foreach (var e in rpf.AllEntries) yield return e;
    if (rpf.Children == null) yield break;
    foreach (var child in rpf.Children)
        foreach (var e in AllEntries(child)) yield return e;
}

static int Api(string filter)
{
    var asm = typeof(RpfFile).Assembly;
    foreach (var t in asm.GetExportedTypes().Where(t => t.Name.Contains(filter)).OrderBy(t => t.FullName))
    {
        Console.WriteLine(t.FullName);
        foreach (var m in t.GetMethods(BindingFlags.Public | BindingFlags.Static | BindingFlags.Instance | BindingFlags.DeclaredOnly))
            Console.WriteLine($"    {(m.IsStatic ? "static " : "")}{m.ReturnType.Name} {m.Name}({string.Join(", ", m.GetParameters().Select(p => p.ParameterType.Name + " " + p.Name))})");
    }
    return 0;
}

// One level archive and the change set that enables it.
record LevelSet(string ChangeSet, string LevelRel, bool Cache, bool Plain, bool Mp, bool Meta, List<string> Ityps);

/// Finds CodeWalker's DLLs (not shipped with cwconv): in CODEWALKER_DIR if
/// set (the installer sets it to the player's CodeWalker folder), else the
/// folder cwconv was built against. Runs before any other code.
static class CodeWalkerLocator
{
    [System.Runtime.CompilerServices.ModuleInitializer]
    internal static void Init()
    {
        var dir = Environment.GetEnvironmentVariable("CODEWALKER_DIR");
        if (string.IsNullOrEmpty(dir))
        {
            dir = typeof(CodeWalkerLocator).Assembly.GetCustomAttributes<AssemblyMetadataAttribute>()
                .FirstOrDefault(a => a.Key == "CodeWalkerDir")?.Value;
        }
        if (string.IsNullOrEmpty(dir) || !File.Exists(Path.Combine(dir, "CodeWalker.Core.dll")))
        {
            Console.Error.WriteLine($"cwconv: CodeWalker not found (no CodeWalker.Core.dll in '{dir}'); set CODEWALKER_DIR to your CodeWalker folder.");
        }
        AppDomain.CurrentDomain.AssemblyResolve += (_, e) =>
        {
            if (string.IsNullOrEmpty(dir)) return null;
            var path = Path.Combine(dir, new AssemblyName(e.Name).Name + ".dll");
            return File.Exists(path) ? Assembly.LoadFrom(path) : null;
        };
    }
}
