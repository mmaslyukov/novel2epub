use epub_builder::{EpubBuilder, EpubContent, ReferenceType, ZipLibrary};
use html_builder::{Buffer, Html5};
use regex::Regex;
use scraper::{Html, Selector};
use std::{
    borrow::Cow,
    fmt::{Display, Write},
    io::{Cursor, Write as OWrite},
    env,
};

#[derive(Debug)]
enum NovelError {
    Http(String),
    Empty,
    Attr(String),
    Selector(String),
    InvalidUrl,
    Image,
    Other(String)
}

impl std::error::Error for NovelError {}

impl Display for NovelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

struct CoverPage {
    page: Html,
}

impl CoverPage {
    fn new(page: Html) -> Self {
        Self { page }
    }

    fn title(&self) -> Result<String, Box<dyn std::error::Error>> {
        // #novel > header > div.header-body.container > div.novel-info > div.main-head > h1
        let selector_path = "h1.novel-title";
        let selector = Selector::parse(selector_path).unwrap();
        let title = self
            .page
            .select(&selector)
            .next()
            .ok_or(Box::new(NovelError::Selector(selector_path.to_string())))?
            .inner_html()
            .as_str()
            .trim()
            .to_string();
        let title = Regex::new(r#"[\\|/|:|"|\n|\r\n|?]{1,}"#)?
            .replace_all(&title, "")
            .to_string();
        let title = Regex::new(r#"\s{2,}"#)?
            .replace_all(title.trim(), "")
            .to_string();
        Ok(title)
    }

    fn author(&self) -> Result<String, Box<dyn std::error::Error>> {
        // #novel > header > div.header-body.container > div.novel-info > div.main-head > div.author > a > span
        let selector_path = "div.author > a > span";
        let selector = Selector::parse(selector_path).unwrap();
        let author = self
            .page
            .select(&selector)
            .next()
            .ok_or(Box::new(NovelError::Selector(selector_path.to_string())))?
            .inner_html()
            .as_str()
            .trim()
            .to_string();
        Ok(author)
    }
    fn cover_img_url(&self) -> Result<String, Box<dyn std::error::Error>> {
        //#novel > header > div.header-body.container > div.fixed-img > figure > img
        // #novel > header > div.header-body.container > div.fixed-img > figure > img
        let selector_path = "div.fixed-img > figure > img";
        let attr_name = "data-src";
        let selector = Selector::parse(selector_path).unwrap();
        let cover_url = self
            .page
            .select(&selector)
            .next()
            .ok_or(Box::new(NovelError::Selector(selector_path.to_string())))?
            .value()
            .attr(attr_name)
            .ok_or(Box::new(NovelError::Attr(attr_name.to_string())))?
            .to_string();
        Ok(cover_url)
    }

    fn cover_img_type(&self) -> Result<String, Box<dyn std::error::Error>> {
        let img_url = self.cover_img_url()?;
        let re = Regex::new(r#"([[:alpha:]]+)(?:\?v=\d*)??$"#)?;
        // let re = Regex::new(r#"([[:alpha:]]+)$"#)?;
        let ext = re
            .captures(&img_url)
            .ok_or(Box::new(NovelError::Image))?
            .get(1)
            .unwrap()
            .as_str()
            .to_string();
        Ok(ext)
    }

    fn chapter_first_url(&self) -> Result<String, Box<dyn std::error::Error>> {
        let selector_path = "#readchapterbtn";
        let attr_name = "href";

        let selector = Selector::parse(selector_path).unwrap();
        let chapter_url = self
            .page
            .select(&selector)
            .next()
            .ok_or(Box::new(NovelError::Selector(selector_path.to_string())))?
            .value()
            .attr(attr_name)
            .ok_or(Box::new(NovelError::Attr(attr_name.to_string())))?
            .to_string();
        Ok(chapter_url)
    }
}

struct ChapterPage {
    page: Html,
}

impl ChapterPage {
    fn new(page: Html) -> Self {
        Self { page }
    }
    fn title(&self) -> Result<String, Box<dyn std::error::Error>> {
        let selector_path = "span.chapter-title";

        let selector = Selector::parse(selector_path).unwrap();
        let title = self
            .page
            .select(&selector)
            .next()
            .ok_or(Box::new(NovelError::Selector(selector_path.to_string())))?
            .inner_html()
            .as_str()
            .trim()
            .to_string();

        let title = Regex::new(r#"[\\|/|:|"|\n|\r\n|?]{1,}"#)?
            .replace_all(&title, "")
            .to_string();
        let title = Regex::new(r#"\s{2,}"#)?
            .replace_all(title.trim(), "")
            .to_string();
        Ok(title)
    }

    fn content(&self) -> Result<String, Box<dyn std::error::Error>> {
        let selector_path = "div.chapter-content";

        let selector = Selector::parse(selector_path).unwrap();
        let content = self
            .page
            .select(&selector)
            .next()
            .ok_or(Box::new(NovelError::Selector(selector_path.to_string())))?
            .inner_html()
            .as_str()
            .trim()
            .to_string();

        Ok(Self::remove_ad(content)?)
    }

    fn chapter_next_url(&self) -> Result<String, Box<dyn std::error::Error>> {
        // #chapter-article > section > div.chapternav.skiptranslate > a.button.nextchap
        let selector_path = "a.button.nextchap";
        let attr_name = "href";

        let selector = Selector::parse(selector_path).unwrap();
        let chapter_url = self
            .page
            .select(&selector)
            .next()
            .ok_or(Box::new(NovelError::Selector(selector_path.to_string())))?
            .value()
            .attr(attr_name)
            .ok_or(Box::new(NovelError::Attr(attr_name.to_string())))?
            .to_string();
        Ok(chapter_url)
    }

    fn compose_xhtml(&self) -> Result<String, Box<dyn std::error::Error>> {
        let mut buf = Buffer::new();
        buf.void_child(Cow::Borrowed("?xml version='1.0' encoding='utf-8'?"));
        buf.doctype();
        let mut html = buf
            .html()
            .attr(r#"xmlns="http://www.w3.org/1999/xhtml""#)
            .attr(r#"xml:lang="en-US""#);
        html.head().raw().write_str(
            r#"<meta http-equiv="Content-Type" content="text/html; charset=utf-8" />"#,
        )?;

        writeln!(html.body().h1(), "{}", self.title()?)?;
        writeln!(html.body().raw(), "{}", self.content()?)?;
        Ok(buf.finish())
    }

    #[inline]
    fn remove_ad(text: String) -> Result<String, Box<dyn std::error::Error>> {
        Ok(Regex::new("<div.*?</div>")?
            .replace_all(&text, "")
            .to_string())
    }
}

struct Novel {
    workdir: String,
    host_url: String,
    cover: CoverPage,
    // title_url: String,
    chapter: Option<ChapterPage>,
    chapter_id: u64,
    // chapter_url: String,
}

impl Novel {
    fn new(title_url: &str, workdir: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            workdir: workdir.to_string(),
            host_url: Self::host(title_url)?,
            cover: CoverPage::new(Self::request_page(title_url)?),
            chapter: None,
            chapter_id: 1
        })
    }

    // fn clear(&self) {
    //     let _ = std::fs::remove_dir_all(format!("{}/{}", self.workdir, self.cover().title().unwrap()));
    // }

    fn host(title_url: &str) -> Result<String, Box<dyn std::error::Error>> {
        let re = Regex::new(r#"https*://[[:alpha:]]+\.[[:alpha:]]+\.[[:alpha:]]+"#)?;
        let capture = re
            .captures_iter(&title_url)
            .next()
            .ok_or(Box::new(NovelError::InvalidUrl))?;
        Ok(capture[0].to_string())
    }

    fn request_page(url: &str) -> Result<Html, Box<dyn std::error::Error>> {
        let resp = reqwest::blocking::get(url)?;
        println!("Request url({}): '{}'", resp.status(), url);
        if resp.status().as_u16() != 200 {
            Err(Box::new(NovelError::Http(
                resp.status().as_str().to_string(),
            )))
        } else {
            let body = resp.text()?;
            Ok(Html::parse_document(&body))
        }
    }
    fn request_data(url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let resp = reqwest::blocking::get(url)?;
        println!("Request url({}): '{}'", resp.status(), url);
        if resp.status().as_u16() != 200 {
            Err(Box::new(NovelError::Http(
                resp.status().as_str().to_string(),
            )))
        } else {
            let data = resp.bytes()?.to_vec();
            Ok(data)
        }
    }

    fn cover(&self) -> &CoverPage {
        &self.cover
    }

    fn chapter(&self) -> Option<&ChapterPage> {
        self.chapter.as_ref()
    }

    fn next(&mut self) -> Option<&ChapterPage> {
        if self.chapter.is_some() {
            self.chapter_id += 1;
            self.chapter = self._chapter_next().ok();
        } else {
            self.chapter = self._chapter_first().ok();
        }
        self.chapter.as_ref()
    }

    fn _chapter_first(&mut self) -> Result<ChapterPage, Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.host_url, self.cover.chapter_first_url()?);
        Ok(ChapterPage::new(Self::request_page(url.as_str())?))
    }

    fn _chapter_next(&mut self) -> Result<ChapterPage, Box<dyn std::error::Error>> {
        let url = format!(
            "{}{}",
            self.host_url,
            self.chapter()
                .as_ref()
                .ok_or(Box::new(NovelError::Empty))?
                .chapter_next_url()?
        );
        Ok(ChapterPage::new(Self::request_page(url.as_str())?))
    }

    fn cover_img_save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let novel_dir = format!("{}/{}", self.workdir, self.cover.title()?);
        std::fs::create_dir_all(&novel_dir)?;
        let img = Self::request_data(self.cover.cover_img_url()?.as_str())?;
        let img_type = self.cover.cover_img_type()?;
        // let data = self.cover.cover_img_url()?;
        let filepath = format!("{novel_dir}/{}.{img_type}", self.cover.title()?);
        println!("Save to '{filepath}'");
        let mut file = std::fs::File::create(filepath)?;
        file.write_all(&img)?;
        Ok(())
    }

    fn chapter_save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let novel_dir = format!("{}/{}", self.workdir, self.cover.title()?);
        std::fs::create_dir_all(&novel_dir)?;
        let xhtml = self
            .chapter
            .as_ref()
            .ok_or(Box::new(NovelError::Empty))?
            .compose_xhtml()?;

        let filepath = format!(
            "{novel_dir}/{:0>8} {}.xhtml",
            self.chapter_id,
            self.chapter
                .as_ref()
                .ok_or(Box::new(NovelError::Empty))?
                .title()?
        );
        println!("Save to '{filepath}'");
        let mut file = std::fs::File::create(filepath)?;
        write!(file, "{}", xhtml)?;
        Ok(())
    }

    fn build_epub(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut builder = EpubBuilder::new(ZipLibrary::new()?)?;
        builder.metadata("author", self.cover.author()?)?;
        builder.metadata("title", self.cover.title()?)?;

        let img_type = self.cover.cover_img_type()?;
        let title = self.cover.title()?;
        let dir = &self.workdir;
        for entry in glob::glob(format!("{dir}/{title}/{title}.{img_type}").as_str())? {
            let path = entry?;
            path.as_path().file_name().unwrap().to_str().unwrap();
            println!("Reading '{}'", path.display());
            let content = std::fs::read(&path)?;
            builder.add_cover_image(
                path.as_path().file_name().unwrap().to_str().unwrap(),
                Cursor::new(content),
                format!("image/{img_type}"),
            )?;
        }

        for entry in glob::glob(format!("{dir}/{}/*.xhtml", self.cover.title()?).as_str())? {
            let path = entry?;
            path.as_path().file_name().unwrap().to_str().unwrap();
            println!("Reading '{}'", path.display());
            let chapter_name = Regex::new(r#"\d*? "#)?.replace(path.as_path().file_name().unwrap().to_str().unwrap(), "");
            let content = std::fs::read_to_string(&path)?;
            builder.add_content(
                EpubContent::new(path.to_str().unwrap(), content.as_bytes())
                    .title(chapter_name)
                    .reftype(ReferenceType::Text),
            )?;
        }
        builder.inline_toc();
        let mut epub: Vec<u8> = vec![];
        builder.generate(&mut epub).unwrap();
        {
            let mut file = std::fs::File::create(format!("{dir}/{}.epub", self.cover().title()?))?;
            file.write_all(&epub)?;
        }

        Ok(())

    }
}


fn validate_arg() -> Result<String, Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    match args.len() {
        2 => {
            let arg: String = args[1].parse()?;
            // Validate URL format
            let _ = Novel::host(&arg)?;
            if Regex::new(r#"lightnovelworld\.com"#)?.is_match(&arg) {
                Ok(arg)
            } else {
                Err(Box::new(NovelError::Other("Only the lightnovelworld.com is supported".to_string())))
            }
        },
        _ => {
            Err(Box::new(NovelError::Other("Please specify novel url".to_string())))
        },
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = validate_arg()?;
    let mut novel = Novel::new(&url, "novel")?;

    // println!("host - {}", Novel::host(url).unwrap());
    // println!("name - {}", novel.cover().title()?);
    // println!("author - {}", novel.cover().author()?);
    // println!("cover_url - {}", novel.cover().cover_img_url()?);
    // println!("chapter_url - {}", novel.cover().chapter_first_url()?);

    // novel.clear();
    novel.cover_img_save()?;
    
    // novel.next();
    // novel.chapter_save()?;
    while novel.next().is_some() {
        novel.chapter_save()?;
    }
    novel.build_epub()?;
    Ok(())
}
