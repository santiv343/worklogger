use hours_core::{TaskHours, Worklog};
use jira_adapter::TeamWorklog;
use printpdf::{
    Actions, Color, ColorArray, FontId, LinePoint, LinkAnnotation, Mm, Op, PaintMode, ParsedFont,
    PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt, Rect,
    Rgb, TextItem, WindingOrder,
};

use crate::copy::text;
use crate::defaults::product_defaults;
use crate::report_analytics::{
    CategoryLabel, CategorySlice, TeamMemberAnalytics, team_member_slices,
};
use crate::report_export::{
    ReportDocument, TeamReportDocument, generated_at, period_label, report_source,
    team_period_label,
};

const REGULAR_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
const BOLD_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
const PAGE_WIDTH_MM: f32 = 210.0;
const PAGE_HEIGHT_MM: f32 = 297.0;
const PAGE_WIDTH_PT: f32 = 595.0;
const PAGE_HEIGHT_PT: f32 = 842.0;
const PAGE_MARGIN_PT: f32 = 36.0;
const HEADER_HEIGHT_PT: f32 = 76.0;
const OVERVIEW_METADATA_Y_PT: f32 = 751.0;
const CARD_TOP_PT: f32 = 735.0;
const CARD_HEIGHT_PT: f32 = 64.0;
const CARD_GAP_PT: f32 = 10.0;
const METRIC_COLUMN_COUNT: usize = 4;
const SECTION_TOP_PT: f32 = 560.0;
const FIRST_BAR_TOP_PT: f32 = 521.0;
const BAR_ROW_HEIGHT_PT: f32 = 38.0;
const BAR_MAX_WIDTH_PT: f32 = 260.0;
const BAR_HEIGHT_PT: f32 = 12.0;
const DETAILS_TOP_PT: f32 = 735.0;
const DETAILS_ROW_HEIGHT_PT: f32 = 58.0;
const DETAILS_ROWS_PER_PAGE: usize = 10;
const MAXIMUM_VISIBLE_LABEL_CHARACTERS: usize = 78;
const LINK_WIDTH_PT: f32 = 78.0;
const LINK_HEIGHT_PT: f32 = 18.0;
const SECONDS_PER_HOUR: f64 = 3_600.0;
const BAR_SCALE: u32 = 10_000;
const CLASSIFICATION_FIRST_TITLE_Y_PT: f32 = 735.0;
const CLASSIFICATION_FIRST_BAR_Y_PT: f32 = 697.0;
const CLASSIFICATION_SECOND_TITLE_Y_PT: f32 = 445.0;
const CLASSIFICATION_SECOND_BAR_Y_PT: f32 = 407.0;

#[derive(Clone)]
struct PdfFonts {
    regular_id: FontId,
    bold_id: FontId,
    regular: ParsedFont,
}

#[derive(Clone, Copy)]
struct PdfColor {
    red: f32,
    green: f32,
    blue: f32,
}

#[derive(Clone, Copy)]
struct PdfTheme {
    primary: PdfColor,
    text: PdfColor,
    muted: PdfColor,
    surface: PdfColor,
    border: PdfColor,
    white: PdfColor,
}

#[derive(Clone)]
struct PdfRecord {
    title: String,
    detail: String,
    value: String,
    url: Option<String>,
}

pub(crate) fn pdf_bytes(document: &ReportDocument) -> Result<Vec<u8>, String> {
    let title = text("reports.export.title");
    let mut pdf = PdfDocument::new(title);
    let fonts = load_fonts(&mut pdf)?;
    let theme = pdf_theme()?;
    let mut pages = vec![personal_overview_page(document, &fonts, theme)];
    append_personal_details(&mut pages, document, &fonts, theme);
    save_pdf(pdf, pages)
}

fn append_personal_details(
    pages: &mut Vec<PdfPage>,
    document: &ReportDocument,
    fonts: &PdfFonts,
    theme: PdfTheme,
) {
    let tasks = task_records(&document.summary.tasks);
    let worklogs = worklog_records(&document.worklogs);
    append_record_pages(pages, text("reports.tasksTitle"), &tasks, fonts, theme);
    append_record_pages(pages, text("reports.sourcesTitle"), &worklogs, fonts, theme);
}

pub(crate) fn team_pdf_bytes(document: &TeamReportDocument) -> Result<Vec<u8>, String> {
    let title = text("reports.export.teamTitle");
    let mut pdf = PdfDocument::new(title);
    let fonts = load_fonts(&mut pdf)?;
    let theme = pdf_theme()?;
    let mut pages = vec![team_overview_page(document, &fonts, theme)];
    if document.analytics.loaded_seconds > 0 {
        pages.push(team_classification_page(document, &fonts, theme));
    }
    append_team_details(&mut pages, document, &fonts, theme);
    save_pdf(pdf, pages)
}

fn append_team_details(
    pages: &mut Vec<PdfPage>,
    document: &TeamReportDocument,
    fonts: &PdfFonts,
    theme: PdfTheme,
) {
    let members = member_records(&document.analytics.members);
    let tasks = task_records(&document.analytics.tasks);
    let worklogs = team_worklog_records(&document.report.worklogs);
    append_record_pages(
        pages,
        text("reports.teamMembersTitle"),
        &members,
        fonts,
        theme,
    );
    append_record_pages(pages, text("reports.tasksTitle"), &tasks, fonts, theme);
    append_record_pages(pages, text("reports.sourcesTitle"), &worklogs, fonts, theme);
}

fn load_fonts(pdf: &mut PdfDocument) -> Result<PdfFonts, String> {
    let regular = parse_font(REGULAR_FONT_BYTES)?;
    let bold = parse_font(BOLD_FONT_BYTES)?;
    Ok(PdfFonts {
        regular_id: pdf.add_font(&regular),
        bold_id: pdf.add_font(&bold),
        regular,
    })
}

fn parse_font(bytes: &[u8]) -> Result<ParsedFont, String> {
    let mut warnings = Vec::new();
    let font = ParsedFont::from_bytes(bytes, 0, &mut warnings)
        .ok_or_else(|| text("reports.export.pdfFontError").to_owned())?;
    if warnings.is_empty() {
        return Ok(font);
    }
    Err(text("reports.export.pdfFontError").to_owned())
}

fn pdf_theme() -> Result<PdfTheme, String> {
    let branding = &product_defaults().branding;
    Ok(PdfTheme {
        primary: parse_color(&branding.primary_color)?,
        text: parse_color(&branding.text_color)?,
        muted: parse_color(&branding.muted_color)?,
        surface: parse_color(&branding.surface_color)?,
        border: parse_color(&branding.border_color)?,
        white: PdfColor::from_rgb(255, 255, 255),
    })
}

fn parse_color(value: &str) -> Result<PdfColor, String> {
    let red = parse_hex_pair(value, 1)?;
    let green = parse_hex_pair(value, 3)?;
    let blue = parse_hex_pair(value, 5)?;
    Ok(PdfColor::from_rgb(red, green, blue))
}

fn parse_hex_pair(value: &str, start: usize) -> Result<u8, String> {
    let end = start.saturating_add(2);
    let pair = value
        .get(start..end)
        .ok_or_else(|| text("reports.export.pdfColorError").to_owned())?;
    u8::from_str_radix(pair, 16).map_err(|_| text("reports.export.pdfColorError").to_owned())
}

impl PdfColor {
    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red: f32::from(red) / 255.0,
            green: f32::from(green) / 255.0,
            blue: f32::from(blue) / 255.0,
        }
    }

    fn operation(self) -> Op {
        Op::SetFillColor {
            col: Color::Rgb(Rgb {
                r: self.red,
                g: self.green,
                b: self.blue,
                icc_profile: None,
            }),
        }
    }
}

fn personal_overview_page(document: &ReportDocument, fonts: &PdfFonts, theme: PdfTheme) -> PdfPage {
    let subtitle = format!(
        "{} · {}",
        period_label(document),
        report_source(&document.source_label, document.context.as_ref())
    );
    let mut operations = page_header(text("reports.export.title"), &subtitle, fonts, theme);
    operations.extend(overview_metadata(document.context.as_ref(), fonts, theme));
    let metrics = personal_metrics(document);
    operations.extend(metric_cards(&metrics, fonts, theme));
    operations.extend(section_heading(text("reports.tasksTitle"), fonts, theme));
    operations.extend(task_bars(&document.summary.tasks, fonts, theme));
    operations.extend(page_footer(1, fonts, theme));
    pdf_page(operations)
}

fn team_overview_page(document: &TeamReportDocument, fonts: &PdfFonts, theme: PdfTheme) -> PdfPage {
    let subtitle = format!(
        "{} · {} · {}",
        team_period_label(document),
        document.filter_summary,
        report_source(&document.source_label, document.context.as_ref())
    );
    let mut operations = page_header(text("reports.export.teamTitle"), &subtitle, fonts, theme);
    operations.extend(overview_metadata(document.context.as_ref(), fonts, theme));
    let metrics = team_metrics(document);
    operations.extend(metric_cards(&metrics, fonts, theme));
    operations.extend(section_heading(
        text("reports.teamMembersChartTitle"),
        fonts,
        theme,
    ));
    operations.extend(team_member_bars(document, fonts, theme));
    operations.extend(page_footer(1, fonts, theme));
    pdf_page(operations)
}

fn team_classification_page(
    document: &TeamReportDocument,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> PdfPage {
    let subtitle = format!(
        "{} · {}",
        team_period_label(document),
        document.filter_summary
    );
    let mut operations = page_header(text("reports.classificationTitle"), &subtitle, fonts, theme);
    operations.extend(issue_type_section(document, fonts, theme));
    operations.extend(issue_status_section(document, fonts, theme));
    operations.extend(page_footer(2, fonts, theme));
    pdf_page(operations)
}

fn issue_type_section(document: &TeamReportDocument, fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    classification_section(
        "reports.issueTypeTitle",
        "reports.unspecifiedType",
        &document.analytics.issue_type_slices,
        CLASSIFICATION_FIRST_TITLE_Y_PT,
        CLASSIFICATION_FIRST_BAR_Y_PT,
        fonts,
        theme,
    )
}

fn issue_status_section(
    document: &TeamReportDocument,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    classification_section(
        "reports.issueStatusTitle",
        "reports.unspecifiedStatus",
        &document.analytics.issue_status_slices,
        CLASSIFICATION_SECOND_TITLE_Y_PT,
        CLASSIFICATION_SECOND_BAR_Y_PT,
        fonts,
        theme,
    )
}

fn classification_section(
    title_key: &'static str,
    empty_label_key: &'static str,
    slices: &[CategorySlice],
    title_y: f32,
    bars_y: f32,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    let mut operations = section_heading_at(text(title_key), title_y, fonts, theme);
    operations.extend(category_bars(slices, empty_label_key, bars_y, fonts, theme));
    operations
}

fn overview_metadata(
    context: Option<&crate::report_export::ReportContext>,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    let offset = context.map_or(0, |value| value.utc_offset_minutes);
    let value = format!(
        "{}: {}",
        text("reports.export.generatedAt"),
        generated_at(offset)
    );
    text_operations(
        &value,
        PAGE_MARGIN_PT,
        OVERVIEW_METADATA_Y_PT,
        7.5,
        false,
        theme.muted,
        fonts,
    )
}

fn personal_metrics(document: &ReportDocument) -> Vec<(String, String)> {
    vec![
        metric(
            "reports.metric.total",
            formatted_hours(document.summary.loaded_seconds),
        ),
        metric(
            "reports.metric.days",
            document.analytics.active_days.to_string(),
        ),
        metric(
            "reports.metric.tasks",
            document.summary.tasks.len().to_string(),
        ),
        metric(
            "reports.metric.entries",
            document.worklogs.len().to_string(),
        ),
        metric(
            "reports.metric.average",
            formatted_hours(document.analytics.average_active_day_seconds),
        ),
        metric("reports.metric.busiest", busiest_day(document)),
        metric(
            "reports.metric.concentration",
            format!("{}%", document.analytics.top_task_percentage),
        ),
        metric(
            "reports.metric.uncommented",
            document.analytics.uncommented_entries.to_string(),
        ),
    ]
}

fn team_metrics(document: &TeamReportDocument) -> Vec<(String, String)> {
    let with_hours = document
        .analytics
        .members
        .iter()
        .filter(|member| member.seconds > 0)
        .count();
    vec![
        metric(
            "reports.teamMetric.total",
            formatted_hours(document.analytics.loaded_seconds),
        ),
        metric(
            "reports.teamMetric.coverage",
            format!("{with_hours}/{}", document.analytics.members.len()),
        ),
        metric(
            "reports.metric.tasks",
            document.analytics.tasks.len().to_string(),
        ),
        metric(
            "reports.metric.entries",
            document.report.worklogs.len().to_string(),
        ),
        metric(
            "reports.metric.uncommented",
            document.analytics.uncommented_entries.to_string(),
        ),
    ]
}

fn metric(label_key: &'static str, value: String) -> (String, String) {
    (text(label_key).to_owned(), value)
}

fn busiest_day(document: &ReportDocument) -> String {
    document.analytics.busiest_day.as_ref().map_or_else(
        || text("reports.noActivity").to_owned(),
        |day| format!("{} · {}", day.date, formatted_hours(day.duration_seconds)),
    )
}

fn page_header(title: &str, subtitle: &str, fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    let mut operations = rectangle(
        0.0,
        PAGE_HEIGHT_PT,
        PAGE_WIDTH_PT,
        HEADER_HEIGHT_PT,
        theme.primary,
    );
    operations.extend(text_operations(
        title,
        PAGE_MARGIN_PT,
        802.0,
        20.0,
        true,
        theme.white,
        fonts,
    ));
    operations.extend(text_operations(
        &fit_text(subtitle),
        PAGE_MARGIN_PT,
        780.0,
        8.5,
        false,
        theme.white,
        fonts,
    ));
    operations
}

fn metric_cards(metrics: &[(String, String)], fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    let column_count = metrics.len().clamp(1, METRIC_COLUMN_COUNT);
    let count = u16::try_from(column_count).unwrap_or(1);
    let width = (PAGE_WIDTH_PT
        - (PAGE_MARGIN_PT * 2.0)
        - (CARD_GAP_PT * f32::from(count.saturating_sub(1))))
        / f32::from(count);
    metrics
        .iter()
        .enumerate()
        .flat_map(|(index, metric)| metric_card(index, metric, width, fonts, theme))
        .collect()
}

fn metric_card(
    index: usize,
    metric: &(String, String),
    width: f32,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    let column = index % METRIC_COLUMN_COUNT;
    let row = index / METRIC_COLUMN_COUNT;
    let x = PAGE_MARGIN_PT + index_position(column) * (width + CARD_GAP_PT);
    let top = CARD_TOP_PT - index_position(row) * (CARD_HEIGHT_PT + CARD_GAP_PT);
    let mut operations = rectangle(x, top, width, CARD_HEIGHT_PT, theme.surface);
    operations.extend(text_operations(
        &metric.0,
        x + 12.0,
        top - 20.0,
        6.5,
        false,
        theme.muted,
        fonts,
    ));
    operations.extend(text_operations(
        &metric.1,
        x + 12.0,
        top - 46.0,
        15.0,
        true,
        theme.primary,
        fonts,
    ));
    operations
}

fn section_heading(title: &str, fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    section_heading_at(title, SECTION_TOP_PT, fonts, theme)
}

fn section_heading_at(title: &str, y: f32, fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    text_operations(title, PAGE_MARGIN_PT, y, 13.0, true, theme.text, fonts)
}

fn category_bars(
    slices: &[CategorySlice],
    empty_label_key: &'static str,
    first_y: f32,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    let maximum = slices
        .iter()
        .map(|slice| slice.seconds)
        .max()
        .unwrap_or_default();
    slices
        .iter()
        .enumerate()
        .flat_map(|(index, slice)| {
            category_bar(
                index,
                slice,
                empty_label_key,
                first_y,
                maximum,
                fonts,
                theme,
            )
        })
        .collect()
}

fn category_bar(
    index: usize,
    slice: &CategorySlice,
    empty_label_key: &'static str,
    first_y: f32,
    maximum: u32,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    let label = pdf_category_label(&slice.label, empty_label_key);
    bar_operations_at(index, first_y, &label, slice.seconds, maximum, fonts, theme)
}

fn pdf_category_label(label: &CategoryLabel, empty_label_key: &'static str) -> String {
    match label {
        CategoryLabel::Value(value) => value.clone(),
        CategoryLabel::Unspecified => text(empty_label_key).to_owned(),
        CategoryLabel::Other => text("reports.otherCategories").to_owned(),
    }
}

fn task_bars(tasks: &[TaskHours], fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    if tasks.is_empty() {
        return empty_state(fonts, theme);
    }
    let maximum = tasks.first().map_or(0, |task| task.duration_seconds);
    tasks
        .iter()
        .take(8)
        .enumerate()
        .flat_map(|(index, task)| task_bar(index, task, maximum, fonts, theme))
        .collect()
}

fn task_bar(
    index: usize,
    task: &TaskHours,
    maximum: u32,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    let y = FIRST_BAR_TOP_PT - index_position(index) * BAR_ROW_HEIGHT_PT;
    let mut operations = text_operations(
        &fit_label(task.issue_key.as_str(), 18),
        PAGE_MARGIN_PT,
        y,
        9.0,
        true,
        theme.text,
        fonts,
    );
    operations.extend(rectangle(
        160.0,
        y + 4.0,
        bar_width(task.duration_seconds, maximum),
        BAR_HEIGHT_PT,
        theme.primary,
    ));
    operations.extend(text_operations(
        &formatted_hours(task.duration_seconds),
        440.0,
        y,
        9.0,
        true,
        theme.primary,
        fonts,
    ));
    operations
}

fn team_member_bars(document: &TeamReportDocument, fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    if document.analytics.loaded_seconds == 0 {
        return empty_state(fonts, theme);
    }
    let maximum_members = product_defaults().reports().maximum_team_chart_members;
    let slices = team_member_slices(&document.analytics.members, maximum_members);
    let maximum = slices
        .iter()
        .map(|slice| slice.seconds)
        .max()
        .unwrap_or_default();
    slices
        .iter()
        .enumerate()
        .flat_map(|(index, slice)| {
            let name = slice
                .display_name
                .as_deref()
                .unwrap_or(text("reports.otherMembers"));
            bar_operations(index, name, slice.seconds, maximum, fonts, theme)
        })
        .collect()
}

fn bar_operations(
    index: usize,
    label: &str,
    seconds: u32,
    maximum: u32,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    bar_operations_at(
        index,
        FIRST_BAR_TOP_PT,
        label,
        seconds,
        maximum,
        fonts,
        theme,
    )
}

fn bar_operations_at(
    index: usize,
    first_y: f32,
    label: &str,
    seconds: u32,
    maximum: u32,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<Op> {
    let y = first_y - index_position(index) * BAR_ROW_HEIGHT_PT;
    let mut operations = text_operations(
        &fit_label(label, 24),
        PAGE_MARGIN_PT,
        y,
        8.5,
        false,
        theme.text,
        fonts,
    );
    operations.extend(rectangle(
        180.0,
        y + 4.0,
        bar_width(seconds, maximum),
        BAR_HEIGHT_PT,
        theme.primary,
    ));
    operations.extend(text_operations(
        &formatted_hours(seconds),
        460.0,
        y,
        8.5,
        true,
        theme.primary,
        fonts,
    ));
    operations
}

fn empty_state(fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    let mut operations = rectangle(
        PAGE_MARGIN_PT,
        530.0,
        PAGE_WIDTH_PT - PAGE_MARGIN_PT * 2.0,
        82.0,
        theme.surface,
    );
    operations.extend(text_operations(
        text("reports.export.emptyTitle"),
        52.0,
        491.0,
        12.0,
        true,
        theme.text,
        fonts,
    ));
    operations.extend(text_operations(
        text("reports.export.emptyDescription"),
        52.0,
        469.0,
        9.0,
        false,
        theme.muted,
        fonts,
    ));
    operations
}

fn record_pages(
    title: &str,
    records: &[PdfRecord],
    first_page: usize,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> Vec<PdfPage> {
    records
        .chunks(DETAILS_ROWS_PER_PAGE)
        .enumerate()
        .map(|(index, chunk)| {
            record_page(title, chunk, index.saturating_add(first_page), fonts, theme)
        })
        .collect()
}

fn append_record_pages(
    pages: &mut Vec<PdfPage>,
    title: &str,
    records: &[PdfRecord],
    fonts: &PdfFonts,
    theme: PdfTheme,
) {
    let first_page = pages.len().saturating_add(1);
    pages.extend(record_pages(title, records, first_page, fonts, theme));
}

fn record_page(
    title: &str,
    records: &[PdfRecord],
    page: usize,
    fonts: &PdfFonts,
    theme: PdfTheme,
) -> PdfPage {
    let mut operations = page_header(title, text("reports.export.detailSubtitle"), fonts, theme);
    for (index, record) in records.iter().enumerate() {
        operations.extend(record_row(index, record, fonts, theme));
    }
    operations.extend(page_footer(page, fonts, theme));
    pdf_page(operations)
}

fn record_row(index: usize, record: &PdfRecord, fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    let top = DETAILS_TOP_PT - index_position(index) * DETAILS_ROW_HEIGHT_PT;
    let fill = if index.is_multiple_of(2) {
        theme.surface
    } else {
        theme.white
    };
    let mut operations = rectangle(
        PAGE_MARGIN_PT,
        top,
        PAGE_WIDTH_PT - PAGE_MARGIN_PT * 2.0,
        DETAILS_ROW_HEIGHT_PT - 4.0,
        fill,
    );
    operations.extend(text_operations(
        &fit_label(&record.title, 52),
        48.0,
        top - 18.0,
        9.0,
        true,
        theme.text,
        fonts,
    ));
    operations.extend(text_operations(
        &fit_text(&record.detail),
        48.0,
        top - 39.0,
        7.8,
        false,
        theme.muted,
        fonts,
    ));
    operations.extend(text_operations(
        &record.value,
        438.0,
        top - 18.0,
        9.0,
        true,
        theme.primary,
        fonts,
    ));
    operations.extend(record_link(record, top, fonts, theme));
    operations
}

fn record_link(record: &PdfRecord, top: f32, fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    let Some(url) = &record.url else {
        return Vec::new();
    };
    let x = 470.0;
    let y = top - 45.0;
    let mut operations = text_operations(
        text("reports.export.openJira"),
        x,
        y + 6.0,
        7.5,
        false,
        theme.primary,
        fonts,
    );
    operations.push(link_operation(
        url,
        x,
        y,
        LINK_WIDTH_PT,
        LINK_HEIGHT_PT,
        theme.primary,
    ));
    operations
}

fn link_operation(url: &str, x: f32, y: f32, width: f32, height: f32, color: PdfColor) -> Op {
    Op::LinkAnnotation {
        link: LinkAnnotation::new(
            Rect::from_xywh(Pt(x), Pt(y), Pt(width), Pt(height)),
            Actions::uri(url.to_owned()),
            None,
            Some(ColorArray::Rgb([color.red, color.green, color.blue])),
            None,
        ),
    }
}

fn task_records(tasks: &[TaskHours]) -> Vec<PdfRecord> {
    tasks
        .iter()
        .map(|task| PdfRecord {
            title: format!("{} · {}", task.issue_key.as_str(), task.summary),
            detail: format!("{}: {}", text("table.entries"), task.entries),
            value: formatted_hours(task.duration_seconds),
            url: Some(task.issue_url.clone()),
        })
        .collect()
}

fn worklog_records(worklogs: &[Worklog]) -> Vec<PdfRecord> {
    worklogs
        .iter()
        .map(|worklog| worklog_record(None, worklog))
        .collect()
}

fn team_worklog_records(worklogs: &[TeamWorklog]) -> Vec<PdfRecord> {
    worklogs.iter().map(team_worklog_record).collect()
}

fn team_worklog_record(entry: &TeamWorklog) -> PdfRecord {
    let metadata = [
        entry.issue_type.as_deref(),
        entry.issue_status.as_deref(),
        entry.assignee_display_name.as_deref(),
        Some(entry.worklog.comment.as_str()),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    let mut record = worklog_record(Some(&entry.author_display_name), &entry.worklog);
    record.detail = metadata;
    record
}

fn worklog_record(member: Option<&str>, worklog: &Worklog) -> PdfRecord {
    let prefix = member.map_or_else(String::new, |name| format!("{name} · "));
    PdfRecord {
        title: format!(
            "{prefix}{} · {}",
            worklog.started.date(),
            worklog.issue_key.as_str()
        ),
        detail: worklog.comment.clone(),
        value: formatted_hours(worklog.duration.seconds()),
        url: Some(worklog.issue_url.clone()),
    }
}

fn member_records(members: &[TeamMemberAnalytics]) -> Vec<PdfRecord> {
    members
        .iter()
        .map(|member| PdfRecord {
            title: member.display_name.clone(),
            detail: format!(
                "{}: {} · {}: {}",
                text("reports.metric.tasks"),
                member.task_count,
                text("table.entries"),
                member.entries
            ),
            value: formatted_hours(member.seconds),
            url: None,
        })
        .collect()
}

fn text_operations(
    value: &str,
    x: f32,
    y: f32,
    size: f32,
    bold: bool,
    color: PdfColor,
    fonts: &PdfFonts,
) -> Vec<Op> {
    vec![
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point { x: Pt(x), y: Pt(y) },
        },
        Op::SetFont {
            font: font_handle(fonts, bold),
            size: Pt(size),
        },
        color.operation(),
        Op::ShowText {
            items: vec![TextItem::Text(supported_text(value, &fonts.regular))],
        },
        Op::EndTextSection,
    ]
}

fn font_handle(fonts: &PdfFonts, bold: bool) -> PdfFontHandle {
    if bold {
        return PdfFontHandle::External(fonts.bold_id.clone());
    }
    PdfFontHandle::External(fonts.regular_id.clone())
}

fn supported_text(value: &str, font: &ParsedFont) -> String {
    value
        .chars()
        .map(|character| {
            if font.lookup_glyph_index(u32::from(character)).is_some() {
                character
            } else {
                '□'
            }
        })
        .collect()
}

fn rectangle(x: f32, top: f32, width: f32, height: f32, color: PdfColor) -> Vec<Op> {
    vec![
        color.operation(),
        Op::DrawPolygon {
            polygon: Polygon {
                rings: vec![PolygonRing {
                    points: rectangle_points(x, top, width, height),
                }],
                mode: PaintMode::Fill,
                winding_order: WindingOrder::NonZero,
            },
        },
    ]
}

fn rectangle_points(x: f32, top: f32, width: f32, height: f32) -> Vec<LinePoint> {
    vec![
        line_point(x, top),
        line_point(x + width, top),
        line_point(x + width, top - height),
        line_point(x, top - height),
    ]
}

fn line_point(x: f32, y: f32) -> LinePoint {
    LinePoint {
        p: Point { x: Pt(x), y: Pt(y) },
        bezier: false,
    }
}

fn page_footer(page: usize, fonts: &PdfFonts, theme: PdfTheme) -> Vec<Op> {
    let label = format!("{} {page}", text("reports.export.page"));
    let mut operations = rectangle(
        PAGE_MARGIN_PT,
        36.0,
        PAGE_WIDTH_PT - PAGE_MARGIN_PT * 2.0,
        1.0,
        theme.border,
    );
    operations.extend(text_operations(
        &label,
        PAGE_WIDTH_PT - 82.0,
        20.0,
        7.5,
        false,
        theme.muted,
        fonts,
    ));
    operations
}

fn pdf_page(operations: Vec<Op>) -> PdfPage {
    PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), operations)
}

fn save_pdf(mut pdf: PdfDocument, pages: Vec<PdfPage>) -> Result<Vec<u8>, String> {
    let mut warnings = Vec::new();
    let bytes = pdf
        .with_pages(pages)
        .save(&PdfSaveOptions::default(), &mut warnings);
    if warnings.is_empty() {
        return Ok(bytes);
    }
    Err(text("reports.export.pdfGenerationError").to_owned())
}

fn fit_text(value: &str) -> String {
    fit_label(
        &value.replace(['\n', '\r'], " "),
        MAXIMUM_VISIBLE_LABEL_CHARACTERS,
    )
}

fn fit_label(value: &str, maximum: usize) -> String {
    let mut characters = value.chars();
    let fitted = characters.by_ref().take(maximum).collect::<String>();
    if characters.next().is_none() {
        return fitted;
    }
    format!("{fitted}…")
}

fn bar_width(seconds: u32, maximum: u32) -> f32 {
    if maximum == 0 {
        return 0.0;
    }
    let scaled = seconds
        .saturating_mul(BAR_SCALE)
        .checked_div(maximum)
        .unwrap_or_default();
    let bounded = u16::try_from(scaled.min(BAR_SCALE)).unwrap_or_default();
    let scale = u16::try_from(BAR_SCALE).unwrap_or(u16::MAX);
    f32::from(bounded) * BAR_MAX_WIDTH_PT / f32::from(scale)
}

fn index_position(index: usize) -> f32 {
    u16::try_from(index).map_or(f32::from(u16::MAX), f32::from)
}

fn formatted_hours(seconds: u32) -> String {
    format!("{:.2} h", f64::from(seconds) / SECONDS_PER_HOUR)
}
