//! FictionBook 2: import as litogramma, kanonizo fixed point,
//! export round trip over the common elements.

use std::path::{Path, PathBuf};

use atrep::{dendron, fb2, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const FB2: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description>
<title-info>
<author><first-name>Александр</first-name><last-name>Пушкин</last-name></author>
<book-title>Капитанская дочка</book-title>
<annotation>
<p>Повесть о пугачёвщине.</p>
</annotation>
</title-info>
</description>
<body>
<title><p>Капитанская дочка</p></title>
<section>
<title><p>Глава I. Сержант гвардии</p></title>
<epigraph>
<p>— Был бы гвардии он завтра ж капитан.</p>
<text-author>Княжнин</text-author>
</epigraph>
<p>Отец мой, Андрей Петрович Гринёв, в молодости своей служил при графе Минихе<a l:href="#n1" type="note">[1]</a> и вышел в отставку <emphasis>премьер-майором</emphasis>.</p>
<empty-line/>
<subtitle>* * *</subtitle>
<p>Матушка была ещё мною <strong>брюхата</strong>.</p>
<poem>
<title><p>Песня</p></title>
<stanza>
<v>Капитанская дочь,</v>
<v>Не ходи гулять в полночь.</v>
</stanza>
<text-author>Народная песня</text-author>
</poem>
<cite>
<p>Береги честь смолоду.</p>
<text-author>Пословица</text-author>
</cite>
<section>
<title><p>Вожатый</p></title>
<p>Сторона ль моя, сторонушка.</p>
</section>
</section>
</body>
<body name="notes">
<section id="n1">
<title><p>1</p></title>
<p>Бурхард Кристоф Миних, генерал-фельдмаршал.</p>
</section>
</body>
</FictionBook>
"##;

#[test]
fn fb2_imports_kanonizes_and_round_trips() {
    let doc = fb2::fb2_to_document(FB2).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.starts_with("@@@!litogramma\n"), "{atd}");
    assert!(atd.contains("@=Капитанская дочка=@"), "{atd}");
    assert!(atd.contains("@=:Александр Пушкин:=@"), "{atd}");
    assert!(atd.contains("@=\"\nПовесть о пугачёвщине.\n\"=@"), "{atd}");
    assert!(atd.contains("@# Глава I. Сержант гвардии\n"), "{atd}");
    assert!(
        atd.contains("@\"/\n— Был бы гвардии он завтра ж капитан.\n/\"@ Княжнин"),
        "{atd}"
    );
    assert!(
        atd.contains("при графе Минихе@^(n1) и вышел в отставку @/премьер-майором/@."),
        "{atd}"
    );
    assert!(atd.contains("@** * * * **@"), "{atd}");
    assert!(atd.contains("@#_* * *_#@"), "{atd}");
    assert!(atd.contains("@*брюхата*@"), "{atd}");
    assert!(
        atd.contains("@~ Песня\nКапитанская дочь,\nНе ходи гулять в полночь.\n~@ Народная песня"),
        "{atd}"
    );
    assert!(
        atd.contains("@\"\nБереги честь смолоду.\n\"@ Пословица"),
        "{atd}"
    );
    assert!(
        atd.contains("@## Вожатый\nСторона ль моя, сторонушка.\n##@"),
        "{atd}"
    );
    assert!(
        atd.contains("@^\nБурхард Кристоф Миних, генерал-фельдмаршал.\n^@(n1)"),
        "{atd}"
    );

    let tmp = tmp_dir("fb2");
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document;
    let atk = dendron::serialize(&kanon);
    std::fs::write(tmp.join("doc2.atd"), &atk).unwrap();
    let kanon2 = kanonizo::kanonizo_file(&tmp.join("doc2.atd"))
        .unwrap()
        .document;
    assert_eq!(dendron::serialize(&kanon2), atk);

    // Export from the kanon: the note onym is canonical now (o1),
    // so the sample comes back with that id and is otherwise
    // identical; the export re-imports to the kanon's own shape.
    let out = fb2::document_to_fb2(&kanon);
    assert_eq!(out, FB2.replace("n1", "o1"));
    let again = fb2::fb2_to_document(&out).unwrap();
    std::fs::write(tmp.join("again.atd"), dendron::serialize(&again)).unwrap();
    let again = kanonizo::kanonizo_file(&tmp.join("again.atd"))
        .unwrap()
        .document;
    assert_eq!(dendron::serialize(&again), atk);
    assert_eq!(fb2::document_to_fb2(&again), out);
}

/// Untitled sections are containers; a lone untitled section
/// holding only loose blocks is the exporter's body wrapper and
/// unwraps on import.
#[test]
fn fb2_untitled_sections() {
    let fb2 = r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description>
<title-info>
<book-title>Стихи</book-title>
</title-info>
</description>
<body>
<title><p>Стихи</p></title>
<section>
<p>Первый.</p>
<section>
<p>Второй.</p>
</section>
</section>
</body>
</FictionBook>
"##;
    let doc = fb2::fb2_to_document(fb2).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@=Стихи=@\n\n@_\nПервый.\n\n@_\nВторой.\n_@\n_@"),
        "{atd}"
    );
    assert_eq!(fb2::document_to_fb2(&doc), fb2);
}

/// Images: the binary comes out of the file as media/<id>, the
/// image block points at it, kanonizo bundles it, and the export
/// writes the image and its binary back.
#[test]
fn fb2_images_round_trip() {
    let png: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
    let b64 = fb2::base64_encode(png);
    assert_eq!(fb2::base64_decode(&b64).unwrap(), png);
    let src = format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description>
<title-info>
<book-title>Картинка</book-title>
</title-info>
</description>
<body>
<title><p>Картинка</p></title>
<section>
<title><p>Глава</p></title>
<image l:href="#pic.png"/>
<p>Подпись.</p>
</section>
</body>
<binary id="pic.png" content-type="image/png">{b64}</binary>
</FictionBook>
"##
    );
    let (doc, media) = fb2::fb2_to_document_with_media(&src).unwrap();
    assert_eq!(media.len(), 1);
    assert_eq!(media[0].id, "pic.png");
    assert_eq!(media[0].bytes, png);
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@# Глава\n@@@@(media/pic.png)\nПодпись.\n#@"),
        "{atd}"
    );

    let tmp = tmp_dir("fb2-images");
    std::fs::create_dir_all(tmp.join("media")).unwrap();
    std::fs::write(tmp.join("media/pic.png"), png).unwrap();
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let atk = dendron::serialize(&result.document);
    assert!(atk.contains("@@@@(media/m1.png)"), "{atk}");
    let out = fb2::document_to_fb2_with(&result.document, &|param| {
        (param == "media/m1.png").then(|| png.to_vec())
    });
    assert!(out.contains(r##"<image l:href="#m1.png"/>"##), "{out}");
    assert!(
        out.contains(&format!(
            r#"<binary id="m1.png" content-type="image/png">{b64}</binary>"#
        )),
        "{out}"
    );
}
