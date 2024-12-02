use std::{
    cell::RefCell,
    fs::File,
    io::{self, Read},
};

use font_loader::system_fonts;
use image::{GenericImage, ImageError, Pixel, Rgb, RgbImage, Rgba};
use ratatui::{
    crossterm::event::{self, KeyCode, KeyEventKind},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
};

use comrak::{
    arena_tree::{Node, NodeEdge},
    nodes::{Ast, NodeValue},
};
use comrak::{parse_document, Arena, Options};
use ratatui_image::{picker::Picker, protocol::Protocol, Image, Resize};
use rusttype::{point, Font, Scale};

fn main() -> io::Result<()> {
    let model = Model::new().map_err::<io::Error, _>(Error::into)?;

    let mut terminal = ratatui::init();
    terminal.clear()?;

    let app_result = run(terminal, model);
    ratatui::restore();
    app_result.map_err(Error::into)
}

struct Model<'a> {
    text: String,
    bg: [u8; 3],
    scroll: i64,
    // root: Box<&'a Node<'a, RefCell<Ast>>>,
    picker: Picker,
    font: Font<'a>,
}

impl<'a> Model<'a> {
    fn new() -> Result<Self, Error> {
        let arena = Arena::new();
        //let md = read_file_to_str("/home/gipsy/o/ratatu-image/README.md")?;
        let text = read_file_to_str("./test.md")?;

        let root = parse_document(&arena, &text, &Options::default());

        let mut picker = Picker::from_query_stdio()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, format!("{err}")))?;

        let property = system_fonts::FontPropertyBuilder::new()
            .monospace()
            //.family(name)
            .family("ProFontWindows Nerd Font Mono")
            .build();
        let (font_data, _) =
            system_fonts::get(&property).ok_or("Could not get system fonts property")?;

        let font = Font::try_from_vec(font_data).ok_or(Error::NoFont)?;

        let bg = [0, 0, 50];
        picker.set_background_color(Some(image::Rgb(bg)));

        Ok(Model {
            text,
            bg,
            scroll: 0,
            // root: Box::new(root),
            picker,
            font,
        })
    }
}

fn run(mut terminal: DefaultTerminal, mut model: Model) -> Result<(), Error> {
    loop {
        terminal.draw(|frame| view(&mut model, frame))?;

        if let event::Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') => {
                        return Ok(());
                    }
                    KeyCode::Char('j') => {
                        model.scroll += 1;
                    }
                    KeyCode::Char('k') => {
                        model.scroll -= 1;
                    }
                    _ => {}
                }
            }
        }
    }
}

fn view(model: &mut Model, frame: &mut Frame) {
    let area = frame.area();
    let block = Block::default()
        .title("Custom Background Block")
        .style(Style::default().bg(Color::Rgb(model.bg[0], model.bg[1], model.bg[2])));
    frame.render_widget(block, area);

    let mut debug = vec![];
    let mut lines = vec![];
    //let mut line: Option<Line> = None;
    let mut spans = vec![];
    let mut style = Style::new();
    let mut y = 0;
    //let mut span = None;

    let arena = Arena::new();
    let root = parse_document(&arena, &model.text, &Options::default());

    for edge in root.traverse() {
        match edge {
            NodeEdge::Start(node) => match node.data.borrow().value {
                ref node_value => {
                    if let CookedModifier::Raw(modifier) = modifier(&node_value) {
                        style = style.add_modifier(modifier);
                    }
                }
            },
            NodeEdge::End(node) => {
                debug.push(Line::from(format!("End {:?}", node.data.borrow().value)));
                match node.data.borrow().value {
                    NodeValue::Text(ref literal) => {
                        let span = Span::from(literal.clone()).style(style);
                        spans.push(span);
                    }
                    NodeValue::Heading(ref tier) => {
                        let widget = Header::new(
                            &mut model.picker,
                            &mut model.font,
                            model.bg,
                            area.width,
                            spans,
                            tier.level,
                        )
                        .unwrap();
                        let height = widget.height;
                        y = render_lines(widget, height, y, area, frame);
                        lines = vec![];
                        spans = vec![];
                    }
                    NodeValue::Image(ref link) => {
                        let widget =
                            LinkImage::new(&mut model.picker, area.width, link.url.as_str());
                        let height = widget.height;
                        y = render_lines(widget, height, y, area, frame);
                        lines = vec![];
                        spans = vec![];
                    }
                    NodeValue::Paragraph => {
                        lines.push(Line::from(spans));
                        lines.push(Line::default());
                        let text = Text::from(lines);
                        lines = vec![];
                        spans = vec![];
                        let height = text.height() as u16;
                        let p = Paragraph::new(text);
                        y = render_lines(p, height, y, area, frame);
                    }
                    NodeValue::LineBreak | NodeValue::SoftBreak => {
                        lines.push(Line::from(spans));
                        let text = Text::from(lines);
                        lines = vec![];
                        spans = vec![];
                        let height = text.height() as u16;
                        let p = Paragraph::new(text);
                        y = render_lines(p, height, y, area, frame);
                    }
                    _ => {
                        if let CookedModifier::Raw(modifier) = modifier(&node.data.borrow().value) {
                            style = style.remove_modifier(modifier);
                        }
                    }
                }
            }
        }
    }
}

struct Header {
    proto: Protocol,
    height: u16,
}

impl<'a> Header {
    fn new(
        picker: &mut Picker,
        font: &mut Font<'a>,
        bg: [u8; 3],
        width: u16,
        spans: Vec<Span>,
        tier: u8,
    ) -> Result<Header, Error> {
        let cell_height = 2;
        let (font_width, font_height) = picker.font_size();
        let img_width = (width * font_width) as u32;
        let img_height = (cell_height * font_height) as u32;
        let img: RgbImage = RgbImage::from_pixel(img_width, img_height, Rgb(bg));
        let mut dyn_img = image::DynamicImage::ImageRgb8(img);

        //let mut spans = spans.clone();
        //spans.push(Span::raw(format!("#{tier}")));
        let s: String = spans.iter().map(|s| s.to_string()).collect();
        let tier_scale = ((12 - tier) as f32) / 12.0f32;
        let scale = Scale::uniform((font_height * cell_height) as f32 * tier_scale);
        let v_metrics = font.v_metrics(scale);
        let glyphs: Vec<_> = font
            .layout(&s, scale, point(0.0, 0.0 + v_metrics.ascent))
            .collect();

        let max_x = img_width as u32;
        let max_y = img_height as u32;
        for glyph in glyphs {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                let mut outside = false;
                let bb_x = bounding_box.min.x as u32;
                let bb_y = bounding_box.min.y as u32;
                glyph.draw(|x, y, v| {
                    let p_x = bb_x + x as u32;
                    let p_y = bb_y + y as u32;
                    if p_x > max_x {
                        outside = true;
                    } else if p_y > max_y {
                        outside = true;
                    } else {
                        let u8v = (255.0 * v) as u8;
                        let mut pixel = Rgba([bg[0], bg[1], bg[2], 255]);
                        pixel.blend(&Rgba([u8v, u8v, u8v, u8v]));
                        dyn_img.put_pixel(p_x, p_y, pixel);
                    }
                });
                if outside {
                    break;
                }
            }
        }

        let proto = picker
            .new_protocol(
                dyn_img,
                Rect::new(0, 0, width, cell_height),
                Resize::Fit(None),
            )
            .unwrap();
        Ok(Header {
            proto,
            height: cell_height,
        })
    }
}

impl Widget for Header {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let image = Image::new(&self.proto);
        image.render(area, buf);
    }
}

struct LinkImage {
    proto: Protocol,
    height: u16,
}

impl LinkImage {
    fn new(picker: &mut Picker, width: u16, link: &str) -> LinkImage {
        let dyn_img = image::ImageReader::open(link).unwrap().decode().unwrap();
        let height: u16 = 10;

        let proto = picker
            .new_protocol(dyn_img, Rect::new(0, 0, width, height), Resize::Fit(None))
            .unwrap();
        LinkImage { proto, height }
    }
}

impl Widget for LinkImage {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let image = Image::new(&self.proto);
        image.render(area, buf);
    }
}

#[derive(Debug)]
enum Error {
    Io(io::Error),
    Image(image::ImageError),
    Msg(String),
    NoFont,
}

impl Into<io::Error> for Error {
    fn into(self) -> io::Error {
        match self {
            Error::Io(io_err) => io_err,
            err => io::Error::new(io::ErrorKind::Other, format!("{err:?}")),
        }
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::Msg(value.to_string())
    }
}

impl From<ImageError> for Error {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

fn render_lines<W: Widget>(widget: W, height: u16, y: u16, area: Rect, f: &mut Frame) -> u16 {
    if y < area.height && area.height - y > height {
        let mut area = area.clone();
        area.y += y;
        area.height = height;
        f.render_widget(widget, area);
    }
    y + height
}

fn read_file_to_str(path: &str) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

enum CookedModifier {
    None,
    Raw(Modifier),
}

fn modifier(node_value: &NodeValue) -> CookedModifier {
    match node_value {
        NodeValue::Strong => CookedModifier::Raw(Modifier::BOLD),
        NodeValue::Emph => CookedModifier::Raw(Modifier::ITALIC),
        NodeValue::Strikethrough => CookedModifier::Raw(Modifier::CROSSED_OUT),
        _ => CookedModifier::None,
    }
}
