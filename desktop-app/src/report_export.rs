use std::{
    io::Write,
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use hours_core::{WeeklySummary, Worklog};
use jira_adapter::{TeamReport, TeamWorklog};
use rust_xlsxwriter::{
    Chart, ChartDataLabel, ChartFormat, ChartLegendPosition, ChartLine, ChartSolidFill,
    ExcelDateTime, Format, FormatAlign, FormatBorder, Url, Workbook, Worksheet, XlsxError,
};
use time::{Date, Duration as TimeDuration, OffsetDateTime, UtcOffset};

use crate::copy::text;
use crate::defaults::product_defaults;
use crate::report_analytics::{
    CategoryLabel, CategorySlice, ReportAnalytics, TaskSlice, TeamAnalytics,
};
use crate::report_pdf::{pdf_bytes, team_pdf_bytes};

const SECONDS_PER_HOUR: f64 = 3_600.0;
const SECONDS_PER_MINUTE: i32 = 60;
const MINUTES_PER_HOUR: u32 = 60;
const XLSX_DASHBOARD_ZOOM: u16 = 90;
const XLSX_TITLE_ROW_HEIGHT: f64 = 30.0;
const XLSX_HEADER_ROW_HEIGHT: f64 = 24.0;
const XLSX_CHART_STYLE: u8 = 10;
const XLSX_CHART_WIDTH: u32 = 620;
const XLSX_CHART_HEIGHT: u32 = 280;
const XLSX_TITLE_LAST_COLUMN: u16 = 7;
const XLSX_TABLE_FIRST_DATA_ROW: u32 = 1;
const XLSX_WEEK_DAYS: i64 = 7;
const XLSX_WEEK_LAST_DAY_OFFSET: i64 = XLSX_WEEK_DAYS - 1;
const XLSX_WEEKLY_CHART_ROW: u32 = 3;
const XLSX_WEEKLY_CHART_COLUMN_GAP: u16 = 2;
const XLSX_SCOPE_TITLE_LAST_COLUMN: u16 = 3;
const XLSX_SCOPE_FIRST_DATA_ROW: u32 = 2;
const XLSX_TASK_CHART_LABEL_COLUMN: u16 = 12;
const XLSX_TASK_CHART_VALUE_COLUMN: u16 = 13;
const XLSX_DASHBOARD_HEADER_ROW: u32 = 5;
const XLSX_DASHBOARD_FIRST_DATA_ROW: u32 = XLSX_DASHBOARD_HEADER_ROW + 1;
const XLSX_PERSONAL_TREND_HEADER_ROW: u32 = 14;
const XLSX_PERSONAL_TREND_FIRST_DATA_ROW: u32 = XLSX_PERSONAL_TREND_HEADER_ROW + 1;
const XLSX_DASHBOARD_SECOND_CHART_COLUMN: u16 = 8;
const XLSX_DASHBOARD_TABLE_GAP: u16 = 2;
const XLSX_DASHBOARD_TABLE_CHART_GAP_ROWS: u32 = 2;
const XLSX_DASHBOARD_CHART_ROW_SPAN: u32 = 16;
const XLSX_DASHBOARD_WEEK_COLUMN_WIDTH: f64 = 25.0;
const XLSX_DASHBOARD_LABEL_COLUMN_WIDTH: f64 = 24.0;
const XLSX_DASHBOARD_VALUE_COLUMN_WIDTH: f64 = 12.0;
const XLSX_DASHBOARD_TASK_COLUMN_WIDTH: f64 = 16.0;
const XLSX_CLASSIFICATION_STATUS_COLUMN: u16 = 3;
const XLSX_CLASSIFICATION_TABLE_GAP_ROWS: u32 = 3;
const XLSX_CLASSIFICATION_SECOND_CHART_COLUMN: u16 = 8;
const XLSX_DAILY_CHART_COLUMN: u16 = 4;
const XLSX_A4_PAPER_SIZE: u8 = 9;
const XLSX_SINGLE_PRINT_PAGE: u16 = 1;
const XLSX_EXTENSION: &str = "xlsx";
const PDF_EXTENSION: &str = "pdf";

#[derive(Clone, Copy)]
struct ReportWeek {
    start: Date,
    end: Date,
}

#[derive(Clone, Copy)]
pub(crate) enum ExportFormat {
    Xlsx,
    Pdf,
}

pub(crate) enum ExportOutcome {
    Cancelled,
    Saved(PathBuf),
}

#[derive(Clone)]
pub(crate) struct ReportContext {
    pub jira_site: String,
    pub board_id: u64,
    pub utc_offset_minutes: i16,
}

#[derive(Clone)]
pub(crate) struct ReportDocument {
    pub identity: String,
    pub source_label: String,
    pub summary: WeeklySummary,
    pub worklogs: Vec<Worklog>,
    pub analytics: ReportAnalytics,
    pub context: Option<ReportContext>,
}

#[derive(Clone)]
pub(crate) struct TeamReportDocument {
    pub(crate) identity: String,
    pub(crate) source_label: String,
    pub(crate) report: TeamReport,
    pub(crate) analytics: TeamAnalytics,
    pub(crate) filter_summary: String,
    pub(crate) total_entries: usize,
    pub(crate) context: Option<ReportContext>,
}

impl TeamReportDocument {
    pub(crate) fn new(
        identity: &str,
        source_label: String,
        report: &TeamReport,
        filter_summary: String,
        total_entries: usize,
        context: Option<ReportContext>,
    ) -> Self {
        Self {
            identity: identity.to_owned(),
            source_label,
            report: report.clone(),
            analytics: TeamAnalytics::calculate(
                report.period,
                &report.members,
                &report.worklogs,
                product_defaults().reports().maximum_task_slices,
            ),
            filter_summary,
            total_entries,
            context,
        }
    }
}

pub(crate) async fn export_report(
    format: ExportFormat,
    document: &ReportDocument,
) -> Result<ExportOutcome, String> {
    let Some(path) = choose_path(format, document).await else {
        return Ok(ExportOutcome::Cancelled);
    };
    write_report(format, &path, document)?;
    Ok(ExportOutcome::Saved(path))
}

pub(crate) async fn export_team_report(
    format: ExportFormat,
    document: &TeamReportDocument,
) -> Result<ExportOutcome, String> {
    let Some(path) = choose_team_path(format, document).await else {
        return Ok(ExportOutcome::Cancelled);
    };
    match format {
        ExportFormat::Xlsx => {
            write_team_xlsx(&path, document).map_err(|error| error.to_string())?;
        }
        ExportFormat::Pdf => {
            write_team_pdf(&path, document)?;
        }
    }
    Ok(ExportOutcome::Saved(path))
}

fn write_report(
    format: ExportFormat,
    path: &Path,
    document: &ReportDocument,
) -> Result<(), String> {
    match format {
        ExportFormat::Xlsx => write_xlsx(path, document).map_err(|error| error.to_string()),
        ExportFormat::Pdf => write_pdf(path, document),
    }
}

async fn choose_path(format: ExportFormat, document: &ReportDocument) -> Option<PathBuf> {
    let extension = format.extension();
    let filename = format!(
        "{}-{}-{}.{}",
        text("reports.export.filePrefix"),
        document.summary.period.start(),
        document.summary.period.end(),
        extension
    );
    let selected = rfd::AsyncFileDialog::new()
        .add_filter(format.label(), &[extension])
        .set_file_name(filename)
        .save_file()
        .await?;
    Some(with_extension(selected.path().to_path_buf(), extension))
}

async fn choose_team_path(format: ExportFormat, document: &TeamReportDocument) -> Option<PathBuf> {
    let extension = format.extension();
    let filename = report_filename(
        format,
        document.report.period.start(),
        document.report.period.end(),
    );
    rfd::AsyncFileDialog::new()
        .add_filter(format.label(), &[extension])
        .set_file_name(filename)
        .save_file()
        .await
        .map(|file| with_extension(file.path().to_path_buf(), extension))
}

fn report_filename(format: ExportFormat, start: time::Date, end: time::Date) -> String {
    format!(
        "{}-equipo-{}-{}.{}",
        text("reports.export.filePrefix"),
        start,
        end,
        format.extension()
    )
}

fn with_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    let already_matches = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|current| current.eq_ignore_ascii_case(extension));
    if already_matches {
        return path;
    }
    path.set_extension(extension);
    path
}

impl ExportFormat {
    const fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => XLSX_EXTENSION,
            Self::Pdf => PDF_EXTENSION,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Xlsx => "Excel",
            Self::Pdf => "PDF",
        }
    }
}

fn write_xlsx(path: &Path, document: &ReportDocument) -> Result<(), XlsxError> {
    save_workbook(path, build_workbook(document)?)
}

fn build_workbook(document: &ReportDocument) -> Result<Workbook, XlsxError> {
    let mut workbook = Workbook::new();
    write_summary_sheet(&mut workbook, document)?;
    write_personal_weekly_sheet(&mut workbook, document)?;
    write_tasks_sheet(&mut workbook, document)?;
    write_worklogs_sheet(&mut workbook, document)?;
    write_personal_scope_sheet(&mut workbook, document)?;
    Ok(workbook)
}

fn write_summary_sheet(
    workbook: &mut Workbook,
    document: &ReportDocument,
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.summarySheet"))?;
    prepare_dashboard_sheet(worksheet)?;
    write_summary_values(worksheet, document)?;
    write_trend_data(worksheet, document)?;
    write_task_chart_data(worksheet, &document.analytics.task_slices)?;
    insert_summary_charts(worksheet, document)?;
    Ok(())
}

fn write_summary_values(
    worksheet: &mut Worksheet,
    document: &ReportDocument,
) -> Result<(), XlsxError> {
    let source = report_source(&document.source_label, document.context.as_ref());
    worksheet.merge_range(
        0,
        0,
        0,
        XLSX_TITLE_LAST_COLUMN,
        text("reports.export.title"),
        &title_format(),
    )?;
    write_label_value(
        worksheet,
        2,
        text("reports.export.account"),
        &document.identity,
    )?;
    write_label_value(worksheet, 3, text("reports.export.source"), &source)?;
    write_label_value(
        worksheet,
        4,
        text("reports.export.period"),
        &period_label(document),
    )?;
    write_label_number(
        worksheet,
        5,
        text("reports.metric.total"),
        hours(document.summary.loaded_seconds),
    )?;
    write_label_number(
        worksheet,
        6,
        text("reports.metric.days"),
        count_as_number(document.analytics.active_days),
    )?;
    write_label_number(
        worksheet,
        7,
        text("reports.metric.tasks"),
        count_as_number(document.summary.tasks.len()),
    )?;
    write_label_number(
        worksheet,
        8,
        text("reports.metric.entries"),
        count_as_number(document.worklogs.len()),
    )?;
    write_label_number(
        worksheet,
        9,
        text("reports.metric.average"),
        hours(document.analytics.average_active_day_seconds),
    )?;
    write_label_value(
        worksheet,
        10,
        text("reports.metric.busiest"),
        &busiest_day_value(document),
    )?;
    write_label_value(
        worksheet,
        11,
        text("reports.metric.concentration"),
        &format!("{}%", document.analytics.top_task_percentage),
    )?;
    write_label_number(
        worksheet,
        12,
        text("reports.metric.uncommented"),
        count_as_number(document.analytics.uncommented_entries),
    )?;
    Ok(())
}

fn busiest_day_value(document: &ReportDocument) -> String {
    document.analytics.busiest_day.as_ref().map_or_else(
        || text("reports.noActivity").to_owned(),
        |day| format!("{} · {}", day.date, formatted_hours(day.duration_seconds)),
    )
}

fn write_label_value(
    worksheet: &mut Worksheet,
    row: u32,
    label: &str,
    value: &str,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, 0, label, &label_format())?;
    worksheet.write_string_with_format(row, 1, value, &value_format())?;
    Ok(())
}

fn write_label_number(
    worksheet: &mut Worksheet,
    row: u32,
    label: &str,
    value: f64,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, 0, label, &label_format())?;
    worksheet.write_number_with_format(row, 1, value, &metric_format())?;
    Ok(())
}

fn write_trend_data(worksheet: &mut Worksheet, document: &ReportDocument) -> Result<(), XlsxError> {
    let header = header_format();
    worksheet.write_string_with_format(
        XLSX_PERSONAL_TREND_HEADER_ROW,
        0,
        text("table.date"),
        &header,
    )?;
    worksheet.write_string_with_format(
        XLSX_PERSONAL_TREND_HEADER_ROW,
        1,
        text("reports.export.dailyHours"),
        &header,
    )?;
    worksheet.write_string_with_format(
        XLSX_PERSONAL_TREND_HEADER_ROW,
        2,
        text("reports.export.cumulativeHours"),
        &header,
    )?;
    for (index, point) in document.analytics.trend.iter().enumerate() {
        let row = xlsx_row(index, XLSX_PERSONAL_TREND_FIRST_DATA_ROW)?;
        worksheet.write_string_with_format(row, 0, point.date.to_string(), &cell_format())?;
        worksheet.write_number_with_format(row, 1, hours(point.seconds), &hours_format())?;
        worksheet.write_number_with_format(
            row,
            2,
            hours(point.cumulative_seconds),
            &hours_format(),
        )?;
    }
    Ok(())
}

fn insert_summary_charts(
    worksheet: &mut Worksheet,
    document: &ReportDocument,
) -> Result<(), XlsxError> {
    insert_trend_chart(worksheet, document)?;
    insert_task_chart(worksheet, document)
}

fn insert_trend_chart(
    worksheet: &mut Worksheet,
    document: &ReportDocument,
) -> Result<(), XlsxError> {
    if document.analytics.trend.is_empty() {
        return Ok(());
    }
    let last_row = xlsx_row(
        document.analytics.trend.len().saturating_sub(1),
        XLSX_PERSONAL_TREND_FIRST_DATA_ROW,
    )?;
    let sheet = text("reports.export.summarySheet");
    let mut trend = Chart::new_line();
    trend
        .add_series()
        .set_categories((sheet, XLSX_PERSONAL_TREND_FIRST_DATA_ROW, 0, last_row, 0))
        .set_values((sheet, XLSX_PERSONAL_TREND_FIRST_DATA_ROW, 1, last_row, 1))
        .set_format(&mut primary_line_format());
    trend.title().set_name(text("reports.trendTitle"));
    style_chart(&mut trend, false);
    worksheet.insert_chart_with_offset(1, 4, &trend, 12, 8)?;
    Ok(())
}

fn insert_task_chart(
    worksheet: &mut Worksheet,
    document: &ReportDocument,
) -> Result<(), XlsxError> {
    if document.analytics.task_slices.is_empty() {
        return Ok(());
    }
    let last_row = xlsx_row(document.analytics.task_slices.len().saturating_sub(1), 0)?;
    let sheet = text("reports.export.summarySheet");
    let mut distribution = Chart::new_doughnut();
    let colors = chart_point_colors(document.analytics.task_slices.len());
    let color_references = colors.iter().map(String::as_str).collect::<Vec<_>>();
    distribution
        .add_series()
        .set_categories((
            sheet,
            0,
            XLSX_TASK_CHART_LABEL_COLUMN,
            last_row,
            XLSX_TASK_CHART_LABEL_COLUMN,
        ))
        .set_values((
            sheet,
            0,
            XLSX_TASK_CHART_VALUE_COLUMN,
            last_row,
            XLSX_TASK_CHART_VALUE_COLUMN,
        ))
        .set_data_label(ChartDataLabel::new().show_percentage())
        .set_point_colors(&color_references);
    distribution
        .title()
        .set_name(text("reports.distributionTitle"));
    distribution.set_hole_size(58);
    style_chart(&mut distribution, true);
    worksheet.insert_chart_with_offset(16, 4, &distribution, 12, 8)?;
    Ok(())
}

fn write_personal_weekly_sheet(
    workbook: &mut Workbook,
    document: &ReportDocument,
) -> Result<(), XlsxError> {
    let weeks = report_weeks(
        document.summary.period.start(),
        document.summary.period.end(),
    );
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.weeklySheet"))?;
    write_weekly_headers(worksheet, &weeks)?;
    write_personal_weekly_values(worksheet, document, &weeks)?;
    finish_weekly_sheet(worksheet, &weeks, 1)?;
    insert_personal_weekly_chart(worksheet, &weeks)?;
    Ok(())
}

fn write_weekly_headers(worksheet: &mut Worksheet, weeks: &[ReportWeek]) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(0, 0, text("reports.member"), &header_format())?;
    for (index, week) in weeks.iter().enumerate() {
        let column = xlsx_column(index, 1)?;
        worksheet.write_string_with_format(0, column, week.label(), &header_format())?;
    }
    worksheet.write_string_with_format(
        0,
        xlsx_column(weeks.len(), 1)?,
        text("reports.export.total"),
        &header_format(),
    )?;
    Ok(())
}

fn write_personal_weekly_values(
    worksheet: &mut Worksheet,
    document: &ReportDocument,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(1, 0, &document.identity, &cell_format())?;
    for (index, week) in weeks.iter().enumerate() {
        worksheet.write_number_with_format(
            1,
            xlsx_column(index, 1)?,
            hours(worklog_seconds_for_week(&document.worklogs, *week)),
            &hours_format(),
        )?;
    }
    worksheet.write_number_with_format(
        1,
        xlsx_column(weeks.len(), 1)?,
        hours(document.summary.loaded_seconds),
        &hours_format(),
    )?;
    Ok(())
}

fn insert_personal_weekly_chart(
    worksheet: &mut Worksheet,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    if weeks.is_empty() {
        return Ok(());
    }
    let last_week_column = xlsx_column(weeks.len().saturating_sub(1), 1)?;
    let mut chart = Chart::new_column();
    chart
        .add_series()
        .set_categories((
            text("reports.export.weeklySheet"),
            0,
            1,
            0,
            last_week_column,
        ))
        .set_values((
            text("reports.export.weeklySheet"),
            1,
            1,
            1,
            last_week_column,
        ))
        .set_data_label(ChartDataLabel::new().show_value())
        .set_format(&mut primary_fill_format());
    chart.title().set_name(text("reports.export.weeklyChart"));
    style_chart(&mut chart, false);
    worksheet.insert_chart(
        XLSX_WEEKLY_CHART_ROW,
        last_week_column.saturating_add(XLSX_WEEKLY_CHART_COLUMN_GAP),
        &chart,
    )?;
    Ok(())
}

fn finish_weekly_sheet(
    worksheet: &mut Worksheet,
    weeks: &[ReportWeek],
    rows: usize,
) -> Result<(), XlsxError> {
    worksheet.set_column_width(0, 32)?;
    worksheet.set_column_range_width(1, xlsx_column(weeks.len(), 1)?, 18)?;
    finish_table(worksheet, rows, xlsx_column(weeks.len(), 1)?)
}

fn report_weeks(start: Date, end: Date) -> Vec<ReportWeek> {
    let weekday_offset = i64::from(start.weekday().number_days_from_monday());
    let mut monday = start - TimeDuration::days(weekday_offset);
    let mut weeks = Vec::new();
    while monday <= end {
        weeks.push(clipped_week(monday, start, end));
        monday += TimeDuration::days(XLSX_WEEK_DAYS);
    }
    weeks
}

fn clipped_week(monday: Date, period_start: Date, period_end: Date) -> ReportWeek {
    let sunday = monday + TimeDuration::days(XLSX_WEEK_LAST_DAY_OFFSET);
    ReportWeek {
        start: monday.max(period_start),
        end: sunday.min(period_end),
    }
}

fn worklog_seconds_for_week(worklogs: &[Worklog], week: ReportWeek) -> u32 {
    worklogs
        .iter()
        .filter(|worklog| week.contains(worklog.started.date()))
        .fold(0, |total, worklog| {
            total.saturating_add(worklog.duration.seconds())
        })
}

impl ReportWeek {
    fn contains(self, date: Date) -> bool {
        date >= self.start && date <= self.end
    }

    fn label(self) -> String {
        format!("{} – {}", self.start, self.end)
    }
}

fn write_tasks_sheet(workbook: &mut Workbook, document: &ReportDocument) -> Result<(), XlsxError> {
    write_task_rows_sheet(workbook, &document.summary.tasks)
}

fn write_task_rows_sheet(
    workbook: &mut Workbook,
    tasks: &[hours_core::TaskHours],
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.tasksSheet"))?;
    write_headers(
        worksheet,
        &[
            text("table.issue"),
            text("table.summary"),
            text("table.entries"),
            text("reports.export.hours"),
            text("reports.export.link"),
        ],
    )?;
    for (index, task) in tasks.iter().enumerate() {
        let row = xlsx_row(index, 1)?;
        worksheet.write_string_with_format(row, 0, task.issue_key.as_str(), &cell_format())?;
        worksheet.write_string_with_format(row, 1, &task.summary, &cell_format())?;
        worksheet.write_number_with_format(
            row,
            2,
            count_as_number(task.entries),
            &integer_format(),
        )?;
        worksheet.write_number_with_format(
            row,
            3,
            hours(task.duration_seconds),
            &hours_format(),
        )?;
        worksheet.write_with_format(
            row,
            4,
            Url::new(&task.issue_url).set_text(text("reports.export.openJira")),
            &link_format(),
        )?;
    }
    set_task_sheet_widths(worksheet)?;
    finish_table(worksheet, tasks.len(), 4)
}

fn write_worklogs_sheet(
    workbook: &mut Workbook,
    document: &ReportDocument,
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.sourcesSheet"))?;
    write_headers(
        worksheet,
        &[
            text("reports.export.worklogId"),
            text("reports.export.accountId"),
            text("table.date"),
            text("reports.export.started"),
            text("reports.export.seconds"),
            text("reports.export.hours"),
            text("table.issue"),
            text("table.summary"),
            text("table.comment"),
            text("reports.export.link"),
        ],
    )?;
    for (index, worklog) in document.worklogs.iter().enumerate() {
        let row = xlsx_row(index, 1)?;
        write_worklog_fields(worksheet, row, 0, worklog)?;
    }
    set_worklog_sheet_widths(worksheet, false)?;
    finish_table(worksheet, document.worklogs.len(), 9)
}

fn write_worklog_fields(
    worksheet: &mut Worksheet,
    row: u32,
    offset: u16,
    worklog: &Worklog,
) -> Result<(), XlsxError> {
    write_worklog_identity(worksheet, row, offset, worklog)?;
    write_worklog_time(worksheet, row, offset + 2, worklog)?;
    write_worklog_issue(worksheet, row, offset + 6, worklog)
}

fn write_worklog_identity(
    worksheet: &mut Worksheet,
    row: u32,
    offset: u16,
    worklog: &Worklog,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, offset, &worklog.id, &cell_format())?;
    worksheet.write_string_with_format(row, offset + 1, worklog.author.as_str(), &cell_format())?;
    Ok(())
}

fn write_worklog_time(
    worksheet: &mut Worksheet,
    row: u32,
    offset: u16,
    worklog: &Worklog,
) -> Result<(), XlsxError> {
    worksheet.write_datetime_with_format(
        row,
        offset,
        &excel_date(worklog.started.date())?,
        &date_format(),
    )?;
    worksheet.write_datetime_with_format(
        row,
        offset + 1,
        &excel_datetime(worklog.started)?,
        &datetime_format(),
    )?;
    worksheet.write_number_with_format(
        row,
        offset + 2,
        f64::from(worklog.duration.seconds()),
        &integer_format(),
    )?;
    worksheet.write_number_with_format(
        row,
        offset + 3,
        hours(worklog.duration.seconds()),
        &hours_format(),
    )?;
    Ok(())
}

fn write_worklog_issue(
    worksheet: &mut Worksheet,
    row: u32,
    offset: u16,
    worklog: &Worklog,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, offset, worklog.issue_key.as_str(), &cell_format())?;
    worksheet.write_string_with_format(row, offset + 1, &worklog.issue_summary, &cell_format())?;
    worksheet.write_string_with_format(row, offset + 2, &worklog.comment, &cell_format())?;
    worksheet.write_with_format(
        row,
        offset + 3,
        Url::new(&worklog.issue_url).set_text(text("reports.export.openJira")),
        &link_format(),
    )?;
    Ok(())
}

fn write_headers(worksheet: &mut Worksheet, labels: &[&str]) -> Result<(), XlsxError> {
    let format = header_format();
    for (column, label) in labels.iter().enumerate() {
        let column = u16::try_from(column).map_err(|_| XlsxError::RowColumnLimitError)?;
        worksheet.write_string_with_format(0, column, *label, &format)?;
    }
    Ok(())
}

fn finish_table(worksheet: &mut Worksheet, rows: usize, last_column: u16) -> Result<(), XlsxError> {
    worksheet.set_freeze_panes(1, 0)?;
    worksheet.set_screen_gridlines(false);
    worksheet.set_row_height(0, XLSX_HEADER_ROW_HEIGHT)?;
    if rows > 0 {
        worksheet.autofilter(0, 0, xlsx_row(rows, 0)?, last_column)?;
    }
    Ok(())
}

fn write_team_xlsx(path: &Path, document: &TeamReportDocument) -> Result<(), XlsxError> {
    save_workbook(path, build_team_workbook(document)?)
}

fn save_workbook(path: &Path, mut workbook: Workbook) -> Result<(), XlsxError> {
    let bytes = workbook.save_to_buffer()?;
    write_atomically(path, &bytes).map_err(XlsxError::IoError)
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = AtomicWriteFile::options().open(path)?;
    file.write_all(bytes)?;
    file.commit()
}

fn build_team_workbook(document: &TeamReportDocument) -> Result<Workbook, XlsxError> {
    let mut workbook = Workbook::new();
    write_team_summary_sheet(&mut workbook, document)?;
    write_team_weekly_sheet(&mut workbook, document)?;
    write_team_daily_sheet(&mut workbook, document)?;
    write_team_members_sheet(&mut workbook, document)?;
    write_team_classification_sheet(&mut workbook, document)?;
    write_team_tasks_sheet(&mut workbook, document)?;
    write_team_sources_sheet(&mut workbook, document)?;
    write_team_scope_sheet(&mut workbook, document)?;
    Ok(workbook)
}

fn write_team_summary_sheet(
    workbook: &mut Workbook,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.summarySheet"))?;
    prepare_dashboard_sheet(worksheet)?;
    write_team_summary_values(worksheet, document)?;
    write_team_dashboard_tables(worksheet, document)?;
    insert_team_charts(worksheet, document)?;
    Ok(())
}

fn write_team_summary_values(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    worksheet.merge_range(
        0,
        0,
        0,
        XLSX_TITLE_LAST_COLUMN,
        text("reports.export.teamTitle"),
        &title_format(),
    )?;
    worksheet.merge_range(
        1,
        0,
        1,
        XLSX_TITLE_LAST_COLUMN,
        &team_dashboard_subtitle(document),
        &subtitle_format(),
    )?;
    write_dashboard_metric(
        worksheet,
        3,
        0,
        text("reports.teamMetric.total"),
        hours(document.analytics.loaded_seconds),
    )?;
    write_dashboard_metric(
        worksheet,
        3,
        2,
        text("reports.teamMetric.members"),
        count_as_number(document.analytics.members.len()),
    )?;
    write_dashboard_metric(
        worksheet,
        3,
        4,
        text("reports.metric.tasks"),
        count_as_number(document.analytics.tasks.len()),
    )?;
    write_dashboard_metric(
        worksheet,
        3,
        6,
        text("reports.metric.entries"),
        count_as_number(document.report.worklogs.len()),
    )?;
    write_dashboard_metric(
        worksheet,
        4,
        0,
        text("reports.teamMetric.coverage"),
        count_as_number(team_members_with_hours(document)),
    )?;
    write_dashboard_metric(
        worksheet,
        4,
        2,
        text("reports.metric.uncommented"),
        count_as_number(document.analytics.uncommented_entries),
    )?;
    Ok(())
}

fn team_members_with_hours(document: &TeamReportDocument) -> usize {
    document
        .analytics
        .members
        .iter()
        .filter(|member| member.seconds > 0)
        .count()
}

fn team_dashboard_subtitle(document: &TeamReportDocument) -> String {
    format!(
        "{} · {} · {}",
        team_period_label(document),
        report_source(&document.source_label, document.context.as_ref()),
        document.filter_summary
    )
}

fn write_dashboard_metric(
    worksheet: &mut Worksheet,
    row: u32,
    column: u16,
    label: &str,
    value: f64,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, column, label, &label_format())?;
    worksheet.write_number_with_format(row, column + 1, value, &metric_format())?;
    Ok(())
}

fn write_team_dashboard_tables(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let weeks = report_weeks(document.report.period.start(), document.report.period.end());
    prepare_team_dashboard_columns(worksheet, &weeks)?;
    write_dashboard_member_table(worksheet, document, &weeks)?;
    write_dashboard_week_table(worksheet, document, &weeks)?;
    write_dashboard_task_table(worksheet, document, &weeks)?;
    Ok(())
}

fn prepare_team_dashboard_columns(
    worksheet: &mut Worksheet,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    let total_column = xlsx_column(weeks.len(), 1)?;
    let week_column = dashboard_week_column(weeks)?;
    let task_column = dashboard_task_column(weeks)?;
    worksheet.set_column_range_width(1, total_column, XLSX_DASHBOARD_WEEK_COLUMN_WIDTH)?;
    worksheet.set_column_width(week_column, XLSX_DASHBOARD_LABEL_COLUMN_WIDTH)?;
    worksheet.set_column_width(week_column + 1, XLSX_DASHBOARD_VALUE_COLUMN_WIDTH)?;
    worksheet.set_column_width(task_column, XLSX_DASHBOARD_TASK_COLUMN_WIDTH)?;
    worksheet.set_column_width(task_column + 1, XLSX_DASHBOARD_VALUE_COLUMN_WIDTH)?;
    Ok(())
}

fn write_dashboard_member_table(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    write_dashboard_member_headers(worksheet, weeks)?;
    let maximum = product_defaults().reports().maximum_team_chart_members;
    for (index, member) in document.analytics.members.iter().take(maximum).enumerate() {
        write_dashboard_member_row(worksheet, document, weeks, index, member)?;
    }
    Ok(())
}

fn write_dashboard_member_headers(
    worksheet: &mut Worksheet,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(
        XLSX_DASHBOARD_HEADER_ROW,
        0,
        text("reports.member"),
        &header_format(),
    )?;
    for (index, week) in weeks.iter().enumerate() {
        worksheet.write_string_with_format(
            XLSX_DASHBOARD_HEADER_ROW,
            xlsx_column(index, 1)?,
            week.label(),
            &header_format(),
        )?;
    }
    worksheet.write_string_with_format(
        XLSX_DASHBOARD_HEADER_ROW,
        xlsx_column(weeks.len(), 1)?,
        text("reports.export.total"),
        &header_format(),
    )?;
    Ok(())
}

fn write_dashboard_member_row(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
    weeks: &[ReportWeek],
    index: usize,
    member: &crate::report_analytics::TeamMemberAnalytics,
) -> Result<(), XlsxError> {
    let row = xlsx_row(index, XLSX_DASHBOARD_FIRST_DATA_ROW)?;
    worksheet.write_string_with_format(row, 0, &member.display_name, &cell_format())?;
    write_member_week_values(worksheet, row, &member.account_id, document, weeks)?;
    worksheet.write_number_with_format(
        row,
        xlsx_column(weeks.len(), 1)?,
        hours(member.seconds),
        &hours_format(),
    )?;
    Ok(())
}

fn write_dashboard_week_table(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    let column = dashboard_week_column(weeks)?;
    write_headers_at(
        worksheet,
        XLSX_DASHBOARD_HEADER_ROW,
        column,
        &[
            text("reports.export.weeklySheet"),
            text("reports.export.hours"),
        ],
    )?;
    for (index, week) in weeks.iter().enumerate() {
        let row = xlsx_row(index, XLSX_DASHBOARD_FIRST_DATA_ROW)?;
        worksheet.write_string_with_format(row, column, week.label(), &cell_format())?;
        worksheet.write_number_with_format(
            row,
            column + 1,
            hours(team_week_seconds(document, *week)),
            &hours_format(),
        )?;
    }
    Ok(())
}

fn write_dashboard_task_table(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    let column = dashboard_task_column(weeks)?;
    write_headers_at(
        worksheet,
        XLSX_DASHBOARD_HEADER_ROW,
        column,
        &[text("table.issue"), text("reports.export.hours")],
    )?;
    for (index, task) in document.analytics.task_slices.iter().enumerate() {
        let row = xlsx_row(index, XLSX_DASHBOARD_FIRST_DATA_ROW)?;
        worksheet.write_string_with_format(
            row,
            column,
            task.issue_key
                .as_deref()
                .unwrap_or(text("reports.otherTasks")),
            &cell_format(),
        )?;
        worksheet.write_number_with_format(
            row,
            column + 1,
            hours(task.seconds),
            &hours_format(),
        )?;
    }
    Ok(())
}

fn dashboard_week_column(weeks: &[ReportWeek]) -> Result<u16, XlsxError> {
    xlsx_column(weeks.len(), XLSX_DASHBOARD_TABLE_GAP + 2)
}

fn dashboard_task_column(weeks: &[ReportWeek]) -> Result<u16, XlsxError> {
    Ok(dashboard_week_column(weeks)? + XLSX_DASHBOARD_TABLE_GAP + 2)
}

fn team_week_seconds(document: &TeamReportDocument, week: ReportWeek) -> u32 {
    document
        .report
        .worklogs
        .iter()
        .filter(|entry| week.contains(entry.worklog.started.date()))
        .fold(0, |total, entry| {
            total.saturating_add(entry.worklog.duration.seconds())
        })
}

fn write_headers_at(
    worksheet: &mut Worksheet,
    row: u32,
    start_column: u16,
    labels: &[&str],
) -> Result<(), XlsxError> {
    for (column, label) in labels.iter().enumerate() {
        let column = xlsx_column(column, start_column)?;
        worksheet.write_string_with_format(row, column, *label, &header_format())?;
    }
    Ok(())
}

fn write_task_chart_data(worksheet: &mut Worksheet, slices: &[TaskSlice]) -> Result<(), XlsxError> {
    for (index, slice) in slices.iter().enumerate() {
        let row = xlsx_row(index, 0)?;
        let label = slice
            .issue_key
            .as_deref()
            .unwrap_or(text("reports.otherTasks"));
        worksheet.write_string(row, XLSX_TASK_CHART_LABEL_COLUMN, label)?;
        worksheet.write_number(row, XLSX_TASK_CHART_VALUE_COLUMN, hours(slice.seconds))?;
    }
    worksheet
        .set_column_range_hidden(XLSX_TASK_CHART_LABEL_COLUMN, XLSX_TASK_CHART_VALUE_COLUMN)?;
    Ok(())
}

fn insert_team_charts(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    insert_team_member_chart(worksheet, document)?;
    insert_team_week_chart(worksheet, document)?;
    insert_team_task_chart(worksheet, document)
}

fn insert_team_task_chart(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    if document.analytics.task_slices.is_empty() || document.analytics.loaded_seconds == 0 {
        return Ok(());
    }
    let last_row = xlsx_row(document.analytics.task_slices.len().saturating_sub(1), 0)?;
    let weeks = report_weeks(document.report.period.start(), document.report.period.end());
    let column = dashboard_task_column(&weeks)?;
    let mut chart = Chart::new_bar();
    chart
        .add_series()
        .set_categories((
            text("reports.export.summarySheet"),
            XLSX_DASHBOARD_FIRST_DATA_ROW,
            column,
            last_row + XLSX_DASHBOARD_FIRST_DATA_ROW,
            column,
        ))
        .set_values((
            text("reports.export.summarySheet"),
            XLSX_DASHBOARD_FIRST_DATA_ROW,
            column + 1,
            last_row + XLSX_DASHBOARD_FIRST_DATA_ROW,
            column + 1,
        ))
        .set_data_label(ChartDataLabel::new().show_value())
        .set_format(&mut primary_fill_format());
    chart.title().set_name(text("reports.distributionTitle"));
    style_chart(&mut chart, false);
    let chart_row = team_dashboard_chart_row(document, &weeks)?;
    let lower_chart_row = chart_row
        .checked_add(XLSX_DASHBOARD_CHART_ROW_SPAN)
        .ok_or(XlsxError::RowColumnLimitError)?;
    worksheet.insert_chart_with_offset(lower_chart_row, 0, &chart, 12, 8)?;
    Ok(())
}

fn insert_team_member_chart(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    if document.analytics.members.is_empty() || document.analytics.loaded_seconds == 0 {
        return Ok(());
    }
    let weeks = report_weeks(document.report.period.start(), document.report.period.end());
    let visible_members = document
        .analytics
        .members
        .len()
        .min(product_defaults().reports().maximum_team_chart_members);
    let last_row = xlsx_row(
        visible_members.saturating_sub(1),
        XLSX_DASHBOARD_FIRST_DATA_ROW,
    )?;
    let total_column = xlsx_column(weeks.len(), 1)?;
    let mut chart = Chart::new_bar();
    chart
        .add_series()
        .set_categories((
            text("reports.export.summarySheet"),
            XLSX_DASHBOARD_FIRST_DATA_ROW,
            0,
            last_row,
            0,
        ))
        .set_values((
            text("reports.export.summarySheet"),
            XLSX_DASHBOARD_FIRST_DATA_ROW,
            total_column,
            last_row,
            total_column,
        ))
        .set_data_label(ChartDataLabel::new().show_value())
        .set_format(&mut primary_fill_format());
    chart
        .title()
        .set_name(text("reports.teamMembersChartTitle"));
    style_chart(&mut chart, false);
    let chart_row = team_dashboard_chart_row(document, &weeks)?;
    worksheet.insert_chart_with_offset(chart_row, 0, &chart, 12, 8)?;
    Ok(())
}

fn insert_team_week_chart(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    if document.analytics.loaded_seconds == 0 {
        return Ok(());
    }
    let weeks = report_weeks(document.report.period.start(), document.report.period.end());
    let last_row = xlsx_row(weeks.len().saturating_sub(1), XLSX_DASHBOARD_FIRST_DATA_ROW)?;
    let column = dashboard_week_column(&weeks)?;
    let mut chart = Chart::new_column();
    chart
        .add_series()
        .set_categories((
            text("reports.export.summarySheet"),
            XLSX_DASHBOARD_FIRST_DATA_ROW,
            column,
            last_row,
            column,
        ))
        .set_values((
            text("reports.export.summarySheet"),
            XLSX_DASHBOARD_FIRST_DATA_ROW,
            column + 1,
            last_row,
            column + 1,
        ))
        .set_data_label(ChartDataLabel::new().show_value())
        .set_format(&mut primary_fill_format());
    chart.title().set_name(text("reports.export.weeklyChart"));
    style_chart(&mut chart, false);
    let chart_row = team_dashboard_chart_row(document, &weeks)?;
    worksheet.insert_chart_with_offset(
        chart_row,
        XLSX_DASHBOARD_SECOND_CHART_COLUMN,
        &chart,
        12,
        8,
    )?;
    Ok(())
}

fn team_dashboard_chart_row(
    document: &TeamReportDocument,
    weeks: &[ReportWeek],
) -> Result<u32, XlsxError> {
    let visible_members = document
        .analytics
        .members
        .len()
        .min(product_defaults().reports().maximum_team_chart_members);
    let table_rows = visible_members
        .max(weeks.len())
        .max(document.analytics.task_slices.len());
    xlsx_row(
        table_rows,
        XLSX_DASHBOARD_HEADER_ROW + XLSX_DASHBOARD_TABLE_CHART_GAP_ROWS,
    )
}

fn write_team_weekly_sheet(
    workbook: &mut Workbook,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let weeks = report_weeks(document.report.period.start(), document.report.period.end());
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.weeklySheet"))?;
    write_weekly_headers(worksheet, &weeks)?;
    write_team_weekly_values(worksheet, document, &weeks)?;
    finish_weekly_sheet(worksheet, &weeks, document.analytics.members.len())?;
    insert_team_weekly_chart(worksheet, document, &weeks)?;
    Ok(())
}

fn write_team_weekly_values(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    for (index, member) in document.analytics.members.iter().enumerate() {
        let row = xlsx_row(index, XLSX_TABLE_FIRST_DATA_ROW)?;
        worksheet.write_string_with_format(row, 0, &member.display_name, &cell_format())?;
        write_member_week_values(worksheet, row, &member.account_id, document, weeks)?;
        worksheet.write_number_with_format(
            row,
            xlsx_column(weeks.len(), 1)?,
            hours(member.seconds),
            &hours_format(),
        )?;
    }
    Ok(())
}

fn write_member_week_values(
    worksheet: &mut Worksheet,
    row: u32,
    account_id: &str,
    document: &TeamReportDocument,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    for (index, week) in weeks.iter().enumerate() {
        worksheet.write_number_with_format(
            row,
            xlsx_column(index, 1)?,
            hours(team_member_week_seconds(document, account_id, *week)),
            &hours_format(),
        )?;
    }
    Ok(())
}

fn team_member_week_seconds(
    document: &TeamReportDocument,
    account_id: &str,
    week: ReportWeek,
) -> u32 {
    document
        .report
        .worklogs
        .iter()
        .filter(|entry| entry.worklog.author.as_str() == account_id)
        .filter(|entry| week.contains(entry.worklog.started.date()))
        .fold(0, |total, entry| {
            total.saturating_add(entry.worklog.duration.seconds())
        })
}

fn insert_team_weekly_chart(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
    weeks: &[ReportWeek],
) -> Result<(), XlsxError> {
    if weeks.is_empty() || document.analytics.members.is_empty() {
        return Ok(());
    }
    let visible_members = document
        .analytics
        .members
        .len()
        .min(product_defaults().reports().maximum_team_chart_members);
    let last_member_row = xlsx_row(visible_members, 0)?;
    let mut chart = Chart::new_column();
    add_team_week_series(&mut chart, weeks, last_member_row)?;
    chart
        .title()
        .set_name(text("reports.export.teamWeeklyChart"));
    style_chart(&mut chart, weeks.len() > 1);
    worksheet.insert_chart(
        XLSX_WEEKLY_CHART_ROW,
        xlsx_column(weeks.len(), XLSX_WEEKLY_CHART_COLUMN_GAP + 1)?,
        &chart,
    )?;
    Ok(())
}

fn add_team_week_series(
    chart: &mut Chart,
    weeks: &[ReportWeek],
    last_member_row: u32,
) -> Result<(), XlsxError> {
    for (index, _) in weeks.iter().enumerate() {
        let column = xlsx_column(index, 1)?;
        let color = chart_color(index);
        chart
            .add_series()
            .set_name((text("reports.export.weeklySheet"), 0, column))
            .set_categories((text("reports.export.weeklySheet"), 1, 0, last_member_row, 0))
            .set_values((
                text("reports.export.weeklySheet"),
                1,
                column,
                last_member_row,
                column,
            ))
            .set_format(&mut series_fill_format(&color));
    }
    Ok(())
}

fn write_team_daily_sheet(
    workbook: &mut Workbook,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.dailySheet"))?;
    write_team_daily_rows(worksheet, document)?;
    insert_team_daily_chart(worksheet, document)?;
    worksheet.set_column_width(0, 16)?;
    worksheet.set_column_range_width(1, 2, 18)?;
    finish_table(worksheet, document.analytics.trend.len(), 2)
}

fn write_team_daily_rows(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    write_headers(
        worksheet,
        &[
            text("table.date"),
            text("reports.export.dailyHours"),
            text("reports.export.cumulativeHours"),
        ],
    )?;
    for (index, point) in document.analytics.trend.iter().enumerate() {
        write_team_daily_row(worksheet, xlsx_row(index, 1)?, point)?;
    }
    Ok(())
}

fn write_team_daily_row(
    worksheet: &mut Worksheet,
    row: u32,
    point: &crate::report_analytics::TrendPoint,
) -> Result<(), XlsxError> {
    worksheet.write_datetime_with_format(row, 0, &excel_date(point.date)?, &date_format())?;
    worksheet.write_number_with_format(row, 1, hours(point.seconds), &hours_format())?;
    worksheet.write_number_with_format(row, 2, hours(point.cumulative_seconds), &hours_format())?;
    Ok(())
}

fn insert_team_daily_chart(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    if document.analytics.trend.is_empty() {
        return Ok(());
    }
    let chart = team_daily_chart(document.analytics.trend.len())?;
    worksheet.insert_chart_with_offset(1, XLSX_DAILY_CHART_COLUMN, &chart, 12, 8)?;
    Ok(())
}

fn team_daily_chart(point_count: usize) -> Result<Chart, XlsxError> {
    let last_row = xlsx_row(point_count.saturating_sub(1), 1)?;
    let sheet = text("reports.export.dailySheet");
    let mut chart = Chart::new_line();
    chart
        .add_series()
        .set_categories((sheet, 1, 0, last_row, 0))
        .set_values((sheet, 1, 1, last_row, 1))
        .set_format(&mut primary_line_format());
    chart.title().set_name(text("reports.trendTitle"));
    style_chart(&mut chart, false);
    Ok(chart)
}

fn write_team_classification_sheet(
    workbook: &mut Workbook,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.classificationSheet"))?;
    prepare_classification_sheet(worksheet)?;
    write_classification_tables(worksheet, document)?;
    insert_classification_charts(worksheet, document)
}

fn write_classification_tables(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    write_category_section(
        worksheet,
        0,
        &document.analytics.issue_type_slices,
        "reports.issueTypeTitle",
        "reports.unspecifiedType",
    )?;
    write_category_section(
        worksheet,
        XLSX_CLASSIFICATION_STATUS_COLUMN,
        &document.analytics.issue_status_slices,
        "reports.issueStatusTitle",
        "reports.unspecifiedStatus",
    )
}

fn prepare_classification_sheet(worksheet: &mut Worksheet) -> Result<(), XlsxError> {
    worksheet.set_screen_gridlines(false);
    worksheet.set_zoom(XLSX_DASHBOARD_ZOOM);
    worksheet.set_column_width(0, 28)?;
    worksheet.set_column_width(1, 14)?;
    worksheet.set_column_width(XLSX_CLASSIFICATION_STATUS_COLUMN, 28)?;
    worksheet.set_column_width(XLSX_CLASSIFICATION_STATUS_COLUMN + 1, 14)?;
    Ok(())
}

fn write_category_section(
    worksheet: &mut Worksheet,
    column: u16,
    slices: &[CategorySlice],
    title_key: &'static str,
    empty_label_key: &'static str,
) -> Result<(), XlsxError> {
    write_headers_at(
        worksheet,
        0,
        column,
        &[text(title_key), text("reports.export.hours")],
    )?;
    for (index, slice) in slices.iter().enumerate() {
        write_category_row(
            worksheet,
            xlsx_row(index, 1)?,
            column,
            slice,
            empty_label_key,
        )?;
    }
    Ok(())
}

fn write_category_row(
    worksheet: &mut Worksheet,
    row: u32,
    column: u16,
    slice: &CategorySlice,
    empty_label_key: &'static str,
) -> Result<(), XlsxError> {
    let label = exported_category_label(&slice.label, empty_label_key);
    worksheet.write_string_with_format(row, column, &label, &cell_format())?;
    worksheet.write_number_with_format(row, column + 1, hours(slice.seconds), &hours_format())?;
    Ok(())
}

fn exported_category_label(label: &CategoryLabel, empty_label_key: &'static str) -> String {
    match label {
        CategoryLabel::Value(value) => value.clone(),
        CategoryLabel::Unspecified => text(empty_label_key).to_owned(),
        CategoryLabel::Other => text("reports.otherCategories").to_owned(),
    }
}

fn insert_classification_charts(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let chart_row = classification_chart_row(document)?;
    insert_issue_type_chart(worksheet, document, chart_row)?;
    insert_issue_status_chart(worksheet, document, chart_row)
}

fn insert_issue_type_chart(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
    chart_row: u32,
) -> Result<(), XlsxError> {
    insert_category_chart(
        worksheet,
        chart_row,
        0,
        &document.analytics.issue_type_slices,
        "reports.issueTypeTitle",
        0,
    )
}

fn insert_issue_status_chart(
    worksheet: &mut Worksheet,
    document: &TeamReportDocument,
    chart_row: u32,
) -> Result<(), XlsxError> {
    insert_category_chart(
        worksheet,
        chart_row,
        XLSX_CLASSIFICATION_STATUS_COLUMN,
        &document.analytics.issue_status_slices,
        "reports.issueStatusTitle",
        XLSX_CLASSIFICATION_SECOND_CHART_COLUMN,
    )
}

fn classification_chart_row(document: &TeamReportDocument) -> Result<u32, XlsxError> {
    let table_rows = document
        .analytics
        .issue_type_slices
        .len()
        .max(document.analytics.issue_status_slices.len());
    xlsx_row(table_rows, XLSX_CLASSIFICATION_TABLE_GAP_ROWS)
}

fn insert_category_chart(
    worksheet: &mut Worksheet,
    chart_row: u32,
    data_column: u16,
    slices: &[CategorySlice],
    title_key: &'static str,
    chart_column: u16,
) -> Result<(), XlsxError> {
    if slices.is_empty() {
        return Ok(());
    }
    let chart = category_chart(data_column, slices.len(), title_key)?;
    worksheet.insert_chart_with_offset(chart_row, chart_column, &chart, 12, 8)?;
    Ok(())
}

fn category_chart(
    data_column: u16,
    slice_count: usize,
    title_key: &'static str,
) -> Result<Chart, XlsxError> {
    let last_row = xlsx_row(slice_count.saturating_sub(1), 1)?;
    let sheet = text("reports.export.classificationSheet");
    let mut chart = Chart::new_bar();
    chart
        .add_series()
        .set_categories((sheet, 1, data_column, last_row, data_column))
        .set_values((sheet, 1, data_column + 1, last_row, data_column + 1))
        .set_data_label(ChartDataLabel::new().show_value())
        .set_format(&mut primary_fill_format());
    chart.title().set_name(text(title_key));
    style_chart(&mut chart, false);
    Ok(chart)
}

fn write_team_members_sheet(
    workbook: &mut Workbook,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.membersSheet"))?;
    write_headers(
        worksheet,
        &[
            text("reports.member"),
            text("reports.coverageState"),
            text("reports.metric.days"),
            text("reports.metric.tasks"),
            text("table.entries"),
            text("reports.export.hours"),
        ],
    )?;
    for (index, member) in document.analytics.members.iter().enumerate() {
        let row = xlsx_row(index, 1)?;
        worksheet.write_string_with_format(row, 0, &member.display_name, &cell_format())?;
        worksheet.write_string_with_format(
            row,
            1,
            exported_member_status(member.seconds),
            &cell_format(),
        )?;
        worksheet.write_number_with_format(
            row,
            2,
            count_as_number(member.active_days),
            &integer_format(),
        )?;
        worksheet.write_number_with_format(
            row,
            3,
            count_as_number(member.task_count),
            &integer_format(),
        )?;
        worksheet.write_number_with_format(
            row,
            4,
            count_as_number(member.entries),
            &integer_format(),
        )?;
        worksheet.write_number_with_format(row, 5, hours(member.seconds), &hours_format())?;
    }
    set_member_sheet_widths(worksheet)?;
    finish_table(worksheet, document.analytics.members.len(), 5)
}

fn exported_member_status(seconds: u32) -> &'static str {
    if seconds == 0 {
        text("reports.coverageNoHours")
    } else {
        text("reports.coverageWithHours")
    }
}

fn write_team_tasks_sheet(
    workbook: &mut Workbook,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.tasksSheet"))?;
    write_headers(worksheet, &team_task_headers())?;
    for (index, task) in document.analytics.tasks.iter().enumerate() {
        write_team_task_row(worksheet, xlsx_row(index, 1)?, task, document)?;
    }
    set_team_task_sheet_widths(worksheet)?;
    finish_table(worksheet, document.analytics.tasks.len(), 7)
}

fn team_task_headers() -> [&'static str; 8] {
    [
        text("table.issue"),
        text("table.summary"),
        text("reports.export.issueType"),
        text("reports.export.status"),
        text("reports.export.assignee"),
        text("table.entries"),
        text("reports.export.hours"),
        text("reports.export.link"),
    ]
}

fn write_team_task_row(
    worksheet: &mut Worksheet,
    row: u32,
    task: &hours_core::TaskHours,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let metadata = task_metadata(document, task.issue_key.as_str());
    worksheet.write_string_with_format(row, 0, task.issue_key.as_str(), &cell_format())?;
    worksheet.write_string_with_format(row, 1, &task.summary, &cell_format())?;
    write_task_metadata(worksheet, row, metadata)?;
    worksheet.write_number_with_format(row, 5, count_as_number(task.entries), &integer_format())?;
    worksheet.write_number_with_format(row, 6, hours(task.duration_seconds), &hours_format())?;
    worksheet.write_with_format(
        row,
        7,
        Url::new(&task.issue_url).set_text(text("reports.export.openJira")),
        &link_format(),
    )?;
    Ok(())
}

fn task_metadata<'a>(document: &'a TeamReportDocument, issue_key: &str) -> Option<&'a TeamWorklog> {
    document
        .report
        .worklogs
        .iter()
        .find(|entry| entry.worklog.issue_key.as_str() == issue_key)
}

fn write_task_metadata(
    worksheet: &mut Worksheet,
    row: u32,
    metadata: Option<&TeamWorklog>,
) -> Result<(), XlsxError> {
    write_optional_string(
        worksheet,
        row,
        2,
        metadata.and_then(|entry| entry.issue_type.as_deref()),
    )?;
    write_optional_string(
        worksheet,
        row,
        3,
        metadata.and_then(|entry| entry.issue_status.as_deref()),
    )?;
    write_optional_string(
        worksheet,
        row,
        4,
        metadata.and_then(|entry| entry.assignee_display_name.as_deref()),
    )
}

fn write_team_sources_sheet(
    workbook: &mut Workbook,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.sourcesSheet"))?;
    write_headers(
        worksheet,
        &[
            text("reports.member"),
            text("reports.export.worklogId"),
            text("reports.export.accountId"),
            text("table.date"),
            text("reports.export.started"),
            text("reports.export.seconds"),
            text("reports.export.hours"),
            text("table.issue"),
            text("table.summary"),
            text("reports.export.issueType"),
            text("reports.export.status"),
            text("reports.export.assignee"),
            text("table.comment"),
            text("reports.export.created"),
            text("reports.export.updated"),
            text("reports.export.link"),
        ],
    )?;
    for (index, entry) in document.report.worklogs.iter().enumerate() {
        write_team_worklog_row(worksheet, xlsx_row(index, 1)?, entry)?;
    }
    set_team_worklog_sheet_widths(worksheet)?;
    finish_table(worksheet, document.report.worklogs.len(), 15)
}

fn write_team_worklog_row(
    worksheet: &mut Worksheet,
    row: u32,
    entry: &TeamWorklog,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, 0, &entry.author_display_name, &cell_format())?;
    write_worklog_identity(worksheet, row, 1, &entry.worklog)?;
    write_worklog_time(worksheet, row, 3, &entry.worklog)?;
    write_team_worklog_issue(worksheet, row, entry)
}

fn write_team_worklog_issue(
    worksheet: &mut Worksheet,
    row: u32,
    entry: &TeamWorklog,
) -> Result<(), XlsxError> {
    let worklog = &entry.worklog;
    worksheet.write_string_with_format(row, 7, worklog.issue_key.as_str(), &cell_format())?;
    worksheet.write_string_with_format(row, 8, &worklog.issue_summary, &cell_format())?;
    write_optional_string(worksheet, row, 9, entry.issue_type.as_deref())?;
    write_optional_string(worksheet, row, 10, entry.issue_status.as_deref())?;
    write_optional_string(worksheet, row, 11, entry.assignee_display_name.as_deref())?;
    write_worklog_audit_fields(worksheet, row, entry)?;
    worksheet.write_with_format(
        row,
        15,
        Url::new(&worklog.issue_url).set_text(text("reports.export.openJira")),
        &link_format(),
    )?;
    Ok(())
}

fn write_worklog_audit_fields(
    worksheet: &mut Worksheet,
    row: u32,
    entry: &TeamWorklog,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, 12, &entry.worklog.comment, &cell_format())?;
    write_optional_string(worksheet, row, 13, entry.created.as_deref())?;
    write_optional_string(worksheet, row, 14, entry.updated.as_deref())
}

fn write_optional_string(
    worksheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: Option<&str>,
) -> Result<(), XlsxError> {
    worksheet.write_string_with_format(row, column, value.unwrap_or_default(), &cell_format())?;
    Ok(())
}

fn write_personal_scope_sheet(
    workbook: &mut Workbook,
    document: &ReportDocument,
) -> Result<(), XlsxError> {
    let rows = personal_scope_rows(document);
    write_scope_sheet(workbook, text("reports.export.title"), &rows)
}

fn personal_scope_rows(document: &ReportDocument) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        (text("reports.export.account"), document.identity.clone()),
        (
            text("reports.export.source"),
            report_source(&document.source_label, document.context.as_ref()),
        ),
        (text("reports.export.period"), period_label(document)),
        (
            text("reports.export.filters"),
            text("reports.export.personalFilter").to_owned(),
        ),
        (
            text("reports.export.resultCount"),
            result_count(document.worklogs.len(), document.worklogs.len()),
        ),
        (
            text("reports.metric.tasks"),
            document.summary.tasks.len().to_string(),
        ),
        (
            text("reports.metric.total"),
            formatted_hours(document.summary.loaded_seconds),
        ),
    ];
    rows.extend(context_scope_rows(document.context.as_ref()));
    rows
}

fn write_team_scope_sheet(
    workbook: &mut Workbook,
    document: &TeamReportDocument,
) -> Result<(), XlsxError> {
    let rows = team_scope_rows(document);
    write_scope_sheet(workbook, text("reports.export.teamTitle"), &rows)
}

fn team_scope_rows(document: &TeamReportDocument) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        (text("reports.export.account"), document.identity.clone()),
        (
            text("reports.export.source"),
            report_source(&document.source_label, document.context.as_ref()),
        ),
        (text("reports.export.period"), team_period_label(document)),
        (
            text("reports.export.rosterScope"),
            text("reports.export.rosterScopeValue").to_owned(),
        ),
        (
            text("reports.export.filters"),
            document.filter_summary.clone(),
        ),
        (
            text("reports.export.resultCount"),
            result_count(document.report.worklogs.len(), document.total_entries),
        ),
        (
            text("reports.export.memberCount"),
            document.analytics.members.len().to_string(),
        ),
        (
            text("reports.metric.tasks"),
            document.analytics.tasks.len().to_string(),
        ),
        (
            text("reports.teamMetric.total"),
            formatted_hours(document.analytics.loaded_seconds),
        ),
    ];
    rows.extend(context_scope_rows(document.context.as_ref()));
    rows
}

pub(crate) fn report_source(label: &str, context: Option<&ReportContext>) -> String {
    let Some(context) = context else {
        return label.to_owned();
    };
    format!(
        "{label} · {} · {} {}",
        context.jira_site,
        text("reports.export.board"),
        context.board_id
    )
}

fn context_scope_rows(context: Option<&ReportContext>) -> Vec<(&'static str, String)> {
    let Some(context) = context else {
        return Vec::new();
    };
    vec![
        (text("reports.export.site"), context.jira_site.clone()),
        (text("reports.export.board"), context.board_id.to_string()),
        (
            text("reports.export.timezone"),
            utc_offset_label(context.utc_offset_minutes),
        ),
        (
            text("reports.export.generatedAt"),
            generated_at(context.utc_offset_minutes),
        ),
    ]
}

fn utc_offset_label(minutes: i16) -> String {
    let absolute = i32::from(minutes).unsigned_abs();
    let sign = if minutes < 0 { "-" } else { "+" };
    format!(
        "UTC{sign}{:02}:{:02}",
        absolute / MINUTES_PER_HOUR,
        absolute % MINUTES_PER_HOUR
    )
}

pub(crate) fn generated_at(offset_minutes: i16) -> String {
    let offset_seconds = i32::from(offset_minutes) * SECONDS_PER_MINUTE;
    let offset = UtcOffset::from_whole_seconds(offset_seconds).unwrap_or(UtcOffset::UTC);
    OffsetDateTime::now_utc().to_offset(offset).to_string()
}

fn write_scope_sheet(
    workbook: &mut Workbook,
    title: &str,
    rows: &[(&str, String)],
) -> Result<(), XlsxError> {
    let worksheet = workbook.add_worksheet();
    worksheet.set_name(text("reports.export.scopeSheet"))?;
    worksheet.merge_range(
        0,
        0,
        0,
        XLSX_SCOPE_TITLE_LAST_COLUMN,
        title,
        &title_format(),
    )?;
    for (index, (label, value)) in rows.iter().enumerate() {
        write_label_value(
            worksheet,
            xlsx_row(index, XLSX_SCOPE_FIRST_DATA_ROW)?,
            label,
            value,
        )?;
    }
    prepare_scope_sheet(worksheet)
}

fn prepare_scope_sheet(worksheet: &mut Worksheet) -> Result<(), XlsxError> {
    worksheet.set_screen_gridlines(false);
    worksheet.set_row_height(0, XLSX_TITLE_ROW_HEIGHT)?;
    worksheet.set_column_width(0, 30)?;
    worksheet.set_column_width(1, 66)?;
    Ok(())
}

fn result_count(included: usize, total: usize) -> String {
    format!("{included} / {total}")
}

fn formatted_hours(seconds: u32) -> String {
    format!("{:.2} {}", hours(seconds), text("unit.hour"))
}

fn xlsx_row(index: usize, offset: u32) -> Result<u32, XlsxError> {
    u32::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(offset))
        .ok_or(XlsxError::RowColumnLimitError)
}

fn xlsx_column(index: usize, offset: u16) -> Result<u16, XlsxError> {
    u16::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(offset))
        .ok_or(XlsxError::RowColumnLimitError)
}

fn title_format() -> Format {
    let branding = &product_defaults().branding;
    Format::new()
        .set_bold()
        .set_font_size(20)
        .set_font_color(branding.primary_color.as_str())
        .set_align(FormatAlign::VerticalCenter)
}

fn subtitle_format() -> Format {
    let branding = &product_defaults().branding;
    Format::new()
        .set_font_color(branding.muted_color.as_str())
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap()
}

fn label_format() -> Format {
    let branding = &product_defaults().branding;
    Format::new()
        .set_bold()
        .set_font_color(branding.text_color.as_str())
        .set_background_color(branding.surface_color.as_str())
        .set_border(FormatBorder::Thin)
        .set_border_color(branding.border_color.as_str())
        .set_align(FormatAlign::VerticalCenter)
}

fn header_format() -> Format {
    let branding = &product_defaults().branding;
    Format::new()
        .set_bold()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_border(FormatBorder::Thin)
        .set_border_color(branding.primary_color.as_str())
        .set_background_color(branding.primary_color.as_str())
        .set_font_color("#FFFFFF")
}

fn value_format() -> Format {
    bordered_format().set_text_wrap()
}

fn metric_format() -> Format {
    let branding = &product_defaults().branding;
    bordered_format()
        .set_bold()
        .set_font_size(14)
        .set_font_color(branding.primary_color.as_str())
        .set_num_format("0.00")
}

fn cell_format() -> Format {
    bordered_format()
        .set_text_wrap()
        .set_align(FormatAlign::Top)
}

fn hours_format() -> Format {
    bordered_format().set_num_format("0.00 \"h\"")
}

fn integer_format() -> Format {
    bordered_format().set_num_format("0")
}

fn date_format() -> Format {
    bordered_format().set_num_format("dd/mm/yyyy")
}

fn datetime_format() -> Format {
    bordered_format().set_num_format("dd/mm/yyyy hh:mm")
}

fn excel_date(date: Date) -> Result<ExcelDateTime, XlsxError> {
    let year =
        u16::try_from(date.year()).map_err(|_| XlsxError::DateTimeRangeError(date.to_string()))?;
    ExcelDateTime::from_ymd(year, u8::from(date.month()), date.day())
}

fn excel_datetime(datetime: OffsetDateTime) -> Result<ExcelDateTime, XlsxError> {
    excel_date(datetime.date())?.and_hms(
        u16::from(datetime.hour()),
        datetime.minute(),
        datetime.second(),
    )
}

fn link_format() -> Format {
    let branding = &product_defaults().branding;
    bordered_format()
        .set_font_color(branding.primary_color.as_str())
        .set_underline(rust_xlsxwriter::FormatUnderline::Single)
}

fn bordered_format() -> Format {
    let branding = &product_defaults().branding;
    Format::new()
        .set_border(FormatBorder::Thin)
        .set_border_color(branding.border_color.as_str())
        .set_align(FormatAlign::VerticalCenter)
}

fn prepare_dashboard_sheet(worksheet: &mut Worksheet) -> Result<(), XlsxError> {
    worksheet.set_screen_gridlines(false);
    worksheet.set_zoom(XLSX_DASHBOARD_ZOOM);
    worksheet.set_landscape();
    worksheet.set_paper_size(XLSX_A4_PAPER_SIZE);
    worksheet.set_print_fit_to_pages(XLSX_SINGLE_PRINT_PAGE, XLSX_SINGLE_PRINT_PAGE);
    worksheet.set_row_height(0, XLSX_TITLE_ROW_HEIGHT)?;
    worksheet.set_column_width(0, 28)?;
    worksheet.set_column_width(1, 30)?;
    worksheet.set_column_width(2, 14)?;
    worksheet.set_column_width(3, 3)?;
    worksheet.set_column_range_width(4, 11, 14)?;
    Ok(())
}

fn style_chart(chart: &mut Chart, show_legend: bool) {
    chart
        .set_style(XLSX_CHART_STYLE)
        .set_width(XLSX_CHART_WIDTH)
        .set_height(XLSX_CHART_HEIGHT);
    if show_legend {
        chart.legend().set_position(ChartLegendPosition::Bottom);
        return;
    }
    chart.legend().set_hidden();
}

fn primary_fill_format() -> ChartFormat {
    series_fill_format(product_defaults().branding.primary_color.as_str())
}

fn primary_line_format() -> ChartFormat {
    let mut format = ChartFormat::new();
    format.set_line(
        ChartLine::new()
            .set_color(product_defaults().branding.primary_color.as_str())
            .set_width(2.5),
    );
    format
}

fn series_fill_format(color: &str) -> ChartFormat {
    let mut format = ChartFormat::new();
    format
        .set_solid_fill(ChartSolidFill::new().set_color(color))
        .set_border(ChartLine::new().set_color(color));
    format
}

fn chart_point_colors(count: usize) -> Vec<String> {
    (0..count).map(chart_color).collect()
}

fn chart_color(index: usize) -> String {
    let defaults = product_defaults();
    let branding = &defaults.branding;
    branding
        .chart_palette
        .get(index % branding.chart_palette.len())
        .unwrap_or(&branding.primary_color)
        .clone()
}

fn set_task_sheet_widths(worksheet: &mut Worksheet) -> Result<(), XlsxError> {
    worksheet.set_column_width(0, 16)?;
    worksheet.set_column_width(1, 52)?;
    worksheet.set_column_range_width(2, 3, 14)?;
    worksheet.set_column_width(4, 18)?;
    Ok(())
}

fn set_team_task_sheet_widths(worksheet: &mut Worksheet) -> Result<(), XlsxError> {
    worksheet.set_column_width(0, 16)?;
    worksheet.set_column_width(1, 52)?;
    worksheet.set_column_range_width(2, 4, 22)?;
    worksheet.set_column_range_width(5, 6, 14)?;
    worksheet.set_column_width(7, 18)?;
    Ok(())
}

fn set_member_sheet_widths(worksheet: &mut Worksheet) -> Result<(), XlsxError> {
    worksheet.set_column_width(0, 32)?;
    worksheet.set_column_range_width(1, 5, 15)?;
    Ok(())
}

fn set_worklog_sheet_widths(
    worksheet: &mut Worksheet,
    includes_member: bool,
) -> Result<(), XlsxError> {
    let offset = u16::from(includes_member);
    if includes_member {
        worksheet.set_column_width(0, 30)?;
    }
    worksheet.set_column_width(offset, 16)?;
    worksheet.set_column_width(offset + 1, 28)?;
    worksheet.set_column_width(offset + 2, 13)?;
    worksheet.set_column_width(offset + 3, 20)?;
    worksheet.set_column_range_width(offset + 4, offset + 5, 14)?;
    worksheet.set_column_width(offset + 6, 16)?;
    worksheet.set_column_width(offset + 7, 48)?;
    worksheet.set_column_width(offset + 8, 42)?;
    worksheet.set_column_width(offset + 9, 18)?;
    Ok(())
}

fn set_team_worklog_sheet_widths(worksheet: &mut Worksheet) -> Result<(), XlsxError> {
    worksheet.set_column_width(0, 30)?;
    worksheet.set_column_width(1, 16)?;
    worksheet.set_column_width(2, 28)?;
    worksheet.set_column_width(3, 13)?;
    worksheet.set_column_width(4, 20)?;
    worksheet.set_column_range_width(5, 6, 14)?;
    worksheet.set_column_width(7, 16)?;
    worksheet.set_column_width(8, 48)?;
    worksheet.set_column_range_width(9, 11, 22)?;
    worksheet.set_column_width(12, 42)?;
    worksheet.set_column_range_width(13, 14, 22)?;
    worksheet.set_column_width(15, 18)?;
    Ok(())
}

fn write_pdf(path: &Path, document: &ReportDocument) -> Result<(), String> {
    let bytes = pdf_bytes(document)?;
    write_atomically(path, &bytes).map_err(|error| error.to_string())
}

fn write_team_pdf(path: &Path, document: &TeamReportDocument) -> Result<(), String> {
    let bytes = team_pdf_bytes(document)?;
    write_atomically(path, &bytes).map_err(|error| error.to_string())
}

pub(crate) fn period_label(document: &ReportDocument) -> String {
    format!(
        "{} - {}",
        document.summary.period.start(),
        document.summary.period.end()
    )
}

pub(crate) fn team_period_label(document: &TeamReportDocument) -> String {
    format!(
        "{} - {}",
        document.report.period.start(),
        document.report.period.end()
    )
}

fn hours(seconds: u32) -> f64 {
    f64::from(seconds) / SECONDS_PER_HOUR
}

fn count_as_number(count: usize) -> f64 {
    u32::try_from(count).map_or(f64::from(u32::MAX), f64::from)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::defaults::product_defaults;
    use hours_core::{AccountId, DateRange, Duration, IssueKey, WeeklyTarget};
    use jira_adapter::{TeamMember, TeamReport};
    use time::{Month, Time};

    const EXPORT_AUDIT_DIRECTORY_VARIABLE: &str = "WORKLOGGER_EXPORT_AUDIT_DIRECTORY";

    #[test]
    fn adds_extension_only_when_missing() {
        assert_eq!(
            with_extension(PathBuf::from("report"), PDF_EXTENSION),
            PathBuf::from("report.pdf")
        );
        assert_eq!(
            with_extension(PathBuf::from("report.xlsx"), PDF_EXTENSION),
            PathBuf::from("report.pdf")
        );
    }

    #[test]
    fn creates_valid_xlsx_and_pdf_documents() {
        let document = personal_document();
        let mut workbook = build_workbook(&document).expect("valid workbook");
        assert_sheet_names(&mut workbook, &personal_sheet_names());
        let xlsx = workbook.save_to_buffer().expect("xlsx bytes");
        assert!(xlsx.starts_with(b"PK"));
        let pdf = pdf_bytes(&document).expect("pdf bytes");
        assert!(pdf.starts_with(b"%PDF"));
        assert!(contains_bytes(&pdf, b"/Subtype/Link"));
        write_audit_artifact("worklogger-personal.xlsx", &xlsx);
        write_audit_artifact("worklogger-personal.pdf", &pdf);
    }

    #[test]
    fn creates_valid_team_xlsx_and_pdf_documents() {
        let report = team_report();
        let document = TeamReportDocument::new(
            "Cuenta de prueba",
            "Jira".to_owned(),
            &report,
            "Sin filtros".to_owned(),
            report.worklogs.len(),
            None,
        );
        let mut workbook = build_team_workbook(&document).expect("valid team workbook");
        assert_sheet_names(&mut workbook, &team_sheet_names());
        let xlsx = workbook.save_to_buffer().expect("team xlsx bytes");
        assert!(xlsx.starts_with(b"PK"));
        let pdf = team_pdf_bytes(&document).expect("team pdf bytes");
        assert!(pdf.starts_with(b"%PDF"));
        assert!(contains_bytes(&pdf, b"/Subtype/Link"));
        write_audit_artifact("worklogger-team.xlsx", &xlsx);
        write_audit_artifact("worklogger-team.pdf", &pdf);
    }

    #[test]
    fn creates_a_single_visible_overview_for_an_empty_team_report() {
        let report = TeamReport {
            period: test_period(),
            members: Vec::new(),
            worklogs: Vec::new(),
            warnings: Vec::new(),
        };
        let document = TeamReportDocument::new(
            "Cuenta de prueba",
            "Jira".to_owned(),
            &report,
            "Sin filtros".to_owned(),
            0,
            None,
        );
        let pdf = team_pdf_bytes(&document).expect("empty team pdf bytes");

        assert_eq!(byte_occurrences(&pdf, b"/Type/Page/"), 1);
        write_audit_artifact("worklogger-team-empty.pdf", &pdf);
    }

    #[test]
    fn splits_partial_period_into_clipped_weeks() {
        let start = test_date(1);
        let end = test_date(9);
        let weeks = report_weeks(start, end);
        assert_eq!(weeks.len(), 2);
        assert_eq!(weeks[0].start, start);
        assert_eq!(weeks[0].end, test_date(6));
        assert_eq!(weeks[1].start, test_date(7));
        assert_eq!(weeks[1].end, end);
    }

    fn personal_document() -> ReportDocument {
        let account = AccountId::new("account-one").expect("valid account");
        let worklogs = vec![test_worklog(&account, 1), test_worklog(&account, 8)];
        let period = test_period();
        let target = WeeklyTarget::from_minutes(1_800).expect("valid target");
        let summary = WeeklySummary::calculate(&account, period, target, &worklogs);
        let analytics = ReportAnalytics::calculate(
            &summary,
            &worklogs,
            product_defaults().reports().maximum_task_slices,
        );
        ReportDocument {
            identity: "Cuenta de prueba".to_owned(),
            source_label: "Jira de prueba".to_owned(),
            summary,
            worklogs,
            analytics,
            context: None,
        }
    }

    fn team_report() -> TeamReport {
        let first = AccountId::new("account-one").expect("valid account");
        let second = AccountId::new("account-two").expect("valid account");
        let third = AccountId::new("account-three").expect("valid account");
        TeamReport {
            period: test_period(),
            members: vec![
                team_member(&first, "Ana López"),
                team_member(&second, "Bruno García"),
                team_member(&third, "Carla Fernández"),
            ],
            worklogs: vec![
                team_worklog(&first, "Ana López", 1),
                team_worklog(&first, "Ana López", 8),
                team_worklog(&second, "Bruno García", 2),
                team_worklog(&second, "Bruno García", 9),
                team_worklog(&third, "Carla Fernández", 3),
                team_worklog(&third, "Carla Fernández", 10),
            ],
            warnings: Vec::new(),
        }
    }

    fn team_member(account: &AccountId, display_name: &str) -> TeamMember {
        TeamMember {
            account_id: account.as_str().to_owned(),
            display_name: display_name.to_owned(),
            active: true,
        }
    }

    fn team_worklog(account: &AccountId, display_name: &str, day: u8) -> TeamWorklog {
        TeamWorklog {
            worklog: test_worklog(account, day),
            author_display_name: display_name.to_owned(),
            issue_type: Some("Tarea".to_owned()),
            issue_status: Some("En curso".to_owned()),
            assignee_display_name: Some(display_name.to_owned()),
            created: Some("2026-09-01T09:00:00.000+0000".to_owned()),
            updated: Some("2026-09-01T10:00:00.000+0000".to_owned()),
        }
    }

    fn test_worklog(account: &AccountId, day: u8) -> Worklog {
        Worklog {
            id: format!("worklog-{day}"),
            issue_key: IssueKey::new(format!("DEMO-{day}")).expect("valid issue"),
            issue_summary: format!("Tarea {day}"),
            author: account.clone(),
            started: test_date(day).with_time(Time::MIDNIGHT).assume_utc(),
            duration: Duration::from_seconds(3_600).expect("valid duration"),
            comment: "Trabajo realizado".to_owned(),
            issue_url: format!("https://example.test/browse/DEMO-{day}"),
        }
    }

    fn test_period() -> DateRange {
        DateRange::new(test_date(1), test_date(14)).expect("valid period")
    }

    fn test_date(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::September, day).expect("valid date")
    }

    fn assert_sheet_names(workbook: &mut Workbook, expected: &[&str]) {
        let names = workbook
            .worksheets()
            .iter()
            .map(Worksheet::name)
            .collect::<Vec<_>>();
        assert_eq!(names, expected);
    }

    fn write_audit_artifact(filename: &str, bytes: &[u8]) {
        let Ok(directory) = std::env::var(EXPORT_AUDIT_DIRECTORY_VARIABLE) else {
            return;
        };
        fs::create_dir_all(&directory).expect("audit directory is created");
        fs::write(PathBuf::from(directory).join(filename), bytes).expect("artifact is written");
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    fn byte_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count()
    }

    fn personal_sheet_names() -> Vec<&'static str> {
        vec![
            text("reports.export.summarySheet"),
            text("reports.export.weeklySheet"),
            text("reports.export.tasksSheet"),
            text("reports.export.sourcesSheet"),
            text("reports.export.scopeSheet"),
        ]
    }

    fn team_sheet_names() -> Vec<&'static str> {
        vec![
            text("reports.export.summarySheet"),
            text("reports.export.weeklySheet"),
            text("reports.export.dailySheet"),
            text("reports.export.membersSheet"),
            text("reports.export.classificationSheet"),
            text("reports.export.tasksSheet"),
            text("reports.export.sourcesSheet"),
            text("reports.export.scopeSheet"),
        ]
    }
}
