//! Classement des fichiers par type à partir de leur extension.

pub const CAT_OTHER: u8 = 0;
pub const CAT_IMAGE: u8 = 1;
pub const CAT_VIDEO: u8 = 2;
pub const CAT_AUDIO: u8 = 3;
pub const CAT_DOCUMENT: u8 = 4;
pub const CAT_ARCHIVE: u8 = 5;
pub const CAT_APP: u8 = 6;
pub const CAT_CODE: u8 = 7;
pub const CAT_SYSTEM: u8 = 8;
pub const CAT_TEMP: u8 = 9;
pub const CAT_DIR: u8 = 255;

pub const CATEGORY_COUNT: usize = 10;

pub fn category_name(cat: u8) -> &'static str {
    match cat {
        CAT_IMAGE => "images",
        CAT_VIDEO => "videos",
        CAT_AUDIO => "audio",
        CAT_DOCUMENT => "documents",
        CAT_ARCHIVE => "archives",
        CAT_APP => "applications",
        CAT_CODE => "code",
        CAT_SYSTEM => "system",
        CAT_TEMP => "temporary",
        CAT_DIR => "folder",
        _ => "other",
    }
}

/// Extension en minuscules (sans le point), limitée à 16 caractères.
pub fn extension(name: &str) -> Option<String> {
    let dot = name.rfind('.')?;
    if dot == 0 || dot + 1 >= name.len() {
        return None;
    }
    let ext = &name[dot + 1..];
    if ext.len() > 16 || ext.contains(' ') {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

pub fn category_of(name: &str) -> u8 {
    let Some(dot) = name.rfind('.') else {
        return CAT_OTHER;
    };
    let ext = &name[dot + 1..];
    if ext.is_empty() || ext.len() > 10 {
        return CAT_OTHER;
    }
    let mut buf = [0u8; 10];
    for (i, b) in ext.bytes().enumerate() {
        buf[i] = b.to_ascii_lowercase();
    }
    let ext = &buf[..ext.len()];
    match ext {
        b"jpg" | b"jpeg" | b"png" | b"gif" | b"bmp" | b"webp" | b"tif" | b"tiff" | b"heic"
        | b"heif" | b"raw" | b"cr2" | b"cr3" | b"nef" | b"arw" | b"dng" | b"psd" | b"svg"
        | b"ico" | b"avif" | b"xcf" | b"kra" | b"exr" | b"hdr" | b"tga" | b"dds" | b"jxl" => {
            CAT_IMAGE
        }
        b"mp4" | b"mkv" | b"avi" | b"mov" | b"wmv" | b"flv" | b"webm" | b"m4v" | b"mpg"
        | b"mpeg" | b"m2ts" | b"mts" | b"3gp" | b"vob" | b"braw" | b"r3d" | b"prproj" => {
            CAT_VIDEO
        }
        b"mp3" | b"wav" | b"flac" | b"aac" | b"ogg" | b"m4a" | b"wma" | b"opus" | b"aiff"
        | b"aif" | b"mid" | b"midi" | b"alac" | b"als" | b"flp" => CAT_AUDIO,
        b"pdf" | b"doc" | b"docx" | b"xls" | b"xlsx" | b"ppt" | b"pptx" | b"odt" | b"ods"
        | b"odp" | b"txt" | b"rtf" | b"md" | b"epub" | b"csv" | b"pages" | b"key"
        | b"numbers" | b"one" | b"pst" | b"ost" | b"xps" | b"mobi" | b"djvu" => CAT_DOCUMENT,
        b"zip" | b"rar" | b"7z" | b"tar" | b"gz" | b"tgz" | b"bz2" | b"xz" | b"zst" | b"iso"
        | b"img" | b"vhd" | b"vhdx" | b"vmdk" | b"vdi" | b"qcow2" | b"wim" | b"esd" | b"cab"
        | b"dmg" | b"lz4" | b"lzma" => CAT_ARCHIVE,
        b"exe" | b"msi" | b"msix" | b"appx" | b"appxbundle" | b"msixbundle" | b"dll" | b"apk"
        | b"jar" | b"pak" | b"ucas" | b"utoc" | b"vpk" | b"bundle" | b"asar" | b"unity3d"
        | b"forge" | b"bsa" | b"ba2" | b"gcf" => CAT_APP,
        b"rs" | b"js" | b"mjs" | b"cjs" | b"ts" | b"tsx" | b"jsx" | b"py" | b"pyc" | b"java"
        | b"class" | b"c" | b"h" | b"cpp" | b"hpp" | b"cc" | b"cs" | b"go" | b"rb" | b"php"
        | b"html" | b"css" | b"scss" | b"json" | b"xml" | b"yml" | b"yaml" | b"toml" | b"lock"
        | b"sql" | b"sh" | b"ps1" | b"bat" | b"vue" | b"svelte" | b"kt" | b"swift" | b"dart"
        | b"lua" | b"map" | b"wasm" | b"pdb" | b"obj" | b"o" | b"a" | b"lib" | b"rlib"
        | b"rmeta" | b"pch" | b"ipch" | b"ilk" | b"node" | b"whl" | b"nupkg" | b"ipynb" => {
            CAT_CODE
        }
        b"sys" | b"drv" | b"mui" | b"cat" | b"inf" | b"etl" | b"evtx" | b"regtrans-ms"
        | b"blf" | b"edb" | b"jrs" | b"chk" | b"dat" | b"db" | b"sdb" | b"manifest" | b"mum"
        | b"efi" | b"ttf" | b"otf" | b"ttc" | b"fon" | b"nls" | b"msp" | b"ocx" | b"cpl"
        | b"scr" | b"winmd" => CAT_SYSTEM,
        b"tmp" | b"temp" | b"log" | b"dmp" | b"mdmp" | b"hdmp" | b"bak" | b"old" | b"cache"
        | b"crdownload" | b"part" | b"partial" | b"download" | b"wer" | b"trace" | b"thumbs"
        | b"swp" | b"~" => CAT_TEMP,
        _ => CAT_OTHER,
    }
}
