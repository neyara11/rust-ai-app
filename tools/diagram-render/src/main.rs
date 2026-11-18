use std::path::{Path, PathBuf};
use std::process::ExitCode;

use charts_rs::{
    svg_to_png, BarChart, HorizontalBarChart, LineChart, PieChart, RadarChart, ScatterChart,
};
use merman::svg::export::{NativeFontContext, NativeFontOptions, RasterOptions};
use merman::{
    MermaidEngine, OperationControl, PngRequest, RenderOutput, RenderRequest, Renderer,
    SourcePresentationConfig, SvgEnvironment, SvgRequest,
};
use serde_json::{json, Map, Value};

fn usage() -> String {
    "usage: diagram-render <mermaid|chart> --input <path> --output <path>".to_string()
}

struct Args {
    kind: String,
    input: PathBuf,
    output: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let kind = args.next().ok_or_else(usage)?;
    if kind != "mermaid" && kind != "chart" {
        return Err(usage());
    }
    let mut input = None;
    let mut output = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--input" => input = args.next().map(PathBuf::from),
            "--output" => output = args.next().map(PathBuf::from),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }
    Ok(Args {
        kind,
        input: input.ok_or_else(|| format!("missing --input\n{}", usage()))?,
        output: output.ok_or_else(|| format!("missing --output\n{}", usage()))?,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let source = match std::fs::read_to_string(&args.input) {
        Ok(source) => source.trim_start_matches('\u{feff}').to_string(),
        Err(error) => {
            eprintln!("cannot read {}: {error}", args.input.display());
            return ExitCode::from(2);
        }
    };
    let result = match args.kind.as_str() {
        "mermaid" => render_mermaid(&source, &args.output),
        "chart" => render_chart(&source, &args.output),
        _ => unreachable!(),
    };
    match result {
        Ok(bytes) => {
            if let Some(parent) = args.output.parent().filter(|p| !p.as_os_str().is_empty()) {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    eprintln!("cannot create {}: {error}", parent.display());
                    return ExitCode::from(1);
                }
            }
            if let Err(error) = std::fs::write(&args.output, bytes) {
                eprintln!("cannot write {}: {error}", args.output.display());
                return ExitCode::from(1);
            }
            println!("ok: {}", args.output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn wants_svg(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("svg"))
        .unwrap_or(false)
}

fn load_fonts(control: &OperationControl) -> Option<NativeFontContext> {
    NativeFontOptions::default()
        .with_system_fonts(true)
        .load(control)
        .ok()
}

fn svg_environment(control: &OperationControl) -> SvgEnvironment {
    let environment = SvgEnvironment::deterministic();
    match load_fonts(control) {
        Some(fonts) => match environment.with_native_font_context(fonts) {
            Ok(environment) => environment,
            Err(_) => SvgEnvironment::deterministic(),
        },
        None => environment,
    }
}

fn mermaid_renderer() -> Renderer {
    let config = SourcePresentationConfig::from_value(json!({
        "theme": "base",
        "fontFamily": "Segoe UI, Arial, sans-serif",
        "themeVariables": {
            "fontFamily": "Segoe UI, Arial, sans-serif",
            "fontSize": "16px",
            "primaryColor": "#e8eefc",
            "primaryBorderColor": "#5b8ff9",
            "primaryTextColor": "#1f2937",
            "lineColor": "#6b7280",
            "secondaryColor": "#f3f4f6",
            "tertiaryColor": "#ffffff"
        }
    }));
    Renderer::new().with_presentation_engine(MermaidEngine::new().with_site_config(config))
}

fn render_mermaid(source: &str, output: &Path) -> Result<Vec<u8>, String> {
    let renderer = mermaid_renderer();
    let control = OperationControl::new();
    if wants_svg(output) {
        let result = renderer
            .render(RenderRequest::svg(
                source,
                control,
                SvgRequest::default(),
            ))
            .map_err(|error| format!("mermaid: {error}"))?;
        let RenderOutput::Svg(Some(svg)) = result else {
            return Err("mermaid: source did not contain a diagram".to_string());
        };
        return Ok(svg.svg().as_bytes().to_vec());
    }
    let raster = RasterOptions::default().with_scale(2.0).with_matte("white");
    let result = renderer
        .render(RenderRequest::png(
            source,
            control.clone(),
            PngRequest {
                svg: SvgRequest {
                    environment: svg_environment(&control),
                    ..Default::default()
                },
                options: raster,
            },
        ))
        .map_err(|error| format!("mermaid: {error}"))?;
    let RenderOutput::Png(Some(png)) = result else {
        return Err("mermaid: source did not contain a diagram".to_string());
    };
    Ok(png.bytes().to_vec())
}

fn render_chart(source: &str, output: &Path) -> Result<Vec<u8>, String> {
    let spec: Value = serde_json::from_str(source)
        .map_err(|error| format!("chart: invalid JSON ({error})"))?;
    let kind = spec
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("bar")
        .to_ascii_lowercase();
    let chart_json = build_chart_json(&kind, &spec)?;
    let svg = chart_svg(&kind, &spec, &chart_json)?;
    if wants_svg(output) {
        return Ok(svg.into_bytes());
    }
    svg_to_png(&svg).map_err(|error| format!("chart: PNG render failed ({error})"))
}

fn chart_svg(kind: &str, spec: &Value, chart_json: &Value) -> Result<String, String> {
    let text = chart_json.to_string();
    let index_axis = spec
        .get("indexAxis")
        .and_then(Value::as_str)
        .map(|value| value.eq_ignore_ascii_case("y"))
        .unwrap_or(false);
    let error = |error: charts_rs::Error| format!("chart: {error}");
    let svg = match kind {
        "pie" => PieChart::from_json(&text).map_err(error)?.svg().map_err(error)?,
        "doughnut" => PieChart::from_json(&text).map_err(error)?.svg().map_err(error)?,
        "radar" => RadarChart::from_json(&text).map_err(error)?.svg().map_err(error)?,
        "scatter" => ScatterChart::from_json(&text).map_err(error)?.svg().map_err(error)?,
        "bar" if index_axis => HorizontalBarChart::from_json(&text)
            .map_err(error)?
            .svg()
            .map_err(error)?,
        "bar" => BarChart::from_json(&text).map_err(error)?.svg().map_err(error)?,
        "line" | "area" => LineChart::from_json(&text).map_err(error)?.svg().map_err(error)?,
        "hbar" => HorizontalBarChart::from_json(&text)
            .map_err(error)?
            .svg()
            .map_err(error)?,
        other => {
            return Err(format!(
                "chart: unsupported type '{other}'. Supported: bar, line, area, pie, doughnut, radar, scatter, hbar"
            ))
        }
    };
    Ok(svg)
}

fn number(value: &Value) -> Option<f32> {
    match value {
        Value::Number(number) => number.as_f64().map(|value| value as f32),
        Value::String(text) => text.trim().replace(',', "").parse::<f32>().ok(),
        _ => None,
    }
}

fn chart_horizontal(spec: &Value) -> bool {
    spec.get("indexAxis")
        .and_then(Value::as_str)
        .map(|axis| axis.eq_ignore_ascii_case("y"))
        .unwrap_or(false)
}

fn chart_title(spec: &Value) -> String {
    if let Some(title) = spec.get("title").and_then(Value::as_str) {
        if !title.trim().is_empty() {
            return title.trim().to_string();
        }
    }
    spec.get("options")
        .and_then(|options| options.get("plugins"))
        .and_then(|plugins| plugins.get("title"))
        .and_then(|title| title.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn chart_stacked(spec: &Value) -> bool {
    if spec.get("stacked").and_then(Value::as_bool).unwrap_or(false) {
        return true;
    }
    ["x", "y"].iter().any(|axis| {
        spec.get("options")
            .and_then(|options| options.get("scales"))
            .and_then(|scales| scales.get(*axis))
            .and_then(|scale| scale.get("stacked"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    })
}

fn build_chart_json(kind: &str, spec: &Value) -> Result<Value, String> {
    let title = chart_title(spec);
    let mut labels: Vec<String> = spec
        .get("labels")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| match item {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    let datasets = spec
        .get("datasets")
        .and_then(Value::as_array)
        .ok_or_else(|| "chart: 'datasets' must be an array".to_string())?;
    if datasets.is_empty() {
        return Err("chart: 'datasets' is empty".to_string());
    }

    if kind == "pie" || kind == "doughnut" {
        return build_pie_json(kind, spec, &title, &labels, datasets);
    }
    if kind == "radar" {
        return build_radar_json(spec, &title, &labels, datasets);
    }

    let mut series = Vec::new();
    let stacked = chart_stacked(spec);
    let horizontal = chart_horizontal(spec);
    for (index, dataset) in datasets.iter().enumerate() {
        let dataset_kind = dataset
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or(kind)
            .to_ascii_lowercase();
        let mut data: Vec<Value> = dataset
            .get("data")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| match value {
                        Value::Object(point) => point
                            .get("y")
                            .and_then(number)
                            .map(|value| json!(value))
                            .unwrap_or(Value::Null),
                        other => number(other).map(|value| json!(value)).unwrap_or(Value::Null),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if horizontal {
            data.reverse();
        }
        let mut item = Map::new();
        item.insert(
            "name".to_string(),
            json!(dataset
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or(&format!("Series {}", index + 1))),
        );
        item.insert("data".to_string(), Value::Array(data));
        if dataset_kind == "line" && kind != "line" {
            item.insert("category".to_string(), json!("line"));
        }
        if dataset_kind == "bar" && (kind == "line" || kind == "area") {
            item.insert("category".to_string(), json!("bar"));
        }
        if stacked || dataset.get("stack").is_some() {
            item.insert("stack".to_string(), json!("total"));
        }
        let axis = dataset
            .get("yAxisID")
            .and_then(Value::as_str)
            .map(|axis| axis.eq_ignore_ascii_case("y1") || axis == "1")
            .unwrap_or(false);
        if axis {
            item.insert("y_axis_index".to_string(), json!(1));
        }
        let filled = kind == "area"
            || dataset_kind == "area"
            || dataset.get("fill").and_then(Value::as_bool).unwrap_or(false);
        if filled {
            item.insert("fill".to_string(), json!(true));
        }
        let smooth = dataset.get("smooth").and_then(Value::as_bool).unwrap_or(false)
            || dataset
                .get("tension")
                .and_then(Value::as_f64)
                .map(|tension| tension > 0.0)
                .unwrap_or(false);
        if smooth {
            item.insert("smooth".to_string(), json!(true));
        }
        series.push(Value::Object(item));
    }

    let mut chart = Map::new();
    chart.insert("width".to_string(), json!(1200));
    chart.insert("height".to_string(), json!(720));
    chart.insert("font_family".to_string(), json!("Roboto"));
    if !title.is_empty() {
        chart.insert("title_text".to_string(), json!(title));
        chart.insert("title_align".to_string(), json!("center"));
    }
    chart.insert("series_list".to_string(), Value::Array(series));
    if horizontal {
        labels.reverse();
    }
    if !labels.is_empty() {
        chart.insert("x_axis_data".to_string(), json!(labels));
    }
    if spec.get("y_axis").is_some() || spec.get("yTitle").is_some() {
        if let Some(y_title) = spec.get("yTitle").and_then(Value::as_str) {
            chart.insert(
                "y_axis_configs".to_string(),
                json!([{ "axis_title": y_title }]),
            );
        }
    }
    if spec.get("y2Title").is_some() {
        let y2 = spec.get("y2Title").and_then(Value::as_str).unwrap_or("");
        chart.insert(
            "y_axis_configs".to_string(),
            json!([
                { "axis_title": spec.get("yTitle").and_then(Value::as_str).unwrap_or("") },
                { "axis_title": y2 }
            ]),
        );
    }
    Ok(Value::Object(chart))
}

fn build_pie_json(
    kind: &str,
    _spec: &Value,
    title: &str,
    labels: &[String],
    datasets: &[Value],
) -> Result<Value, String> {
    let dataset = &datasets[0];
    let values = dataset
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "chart: pie dataset must contain 'data'".to_string())?;
    let mut series = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let name = labels
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("Item {}", index + 1));
        series.push(json!({ "name": name, "data": [number(value).unwrap_or(0.0)] }));
    }
    let mut chart = Map::new();
    chart.insert("width".to_string(), json!(1200));
    chart.insert("height".to_string(), json!(720));
    chart.insert("font_family".to_string(), json!("Roboto"));
    chart.insert("series_list".to_string(), Value::Array(series));
    chart.insert("legend_align".to_string(), json!("right"));
    chart.insert("rose_type".to_string(), json!(false));
    chart.insert("border_radius".to_string(), json!(0));
    let heading = if title.is_empty() {
        dataset.get("label").and_then(Value::as_str).unwrap_or("")
    } else {
        title
    };
    if !heading.is_empty() {
        chart.insert("title_text".to_string(), json!(heading));
        chart.insert("title_align".to_string(), json!("center"));
    }
    if kind == "doughnut" {
        chart.insert("inner_radius".to_string(), json!(0.55));
    }
    Ok(Value::Object(chart))
}

fn build_radar_json(
    _spec: &Value,
    title: &str,
    labels: &[String],
    datasets: &[Value],
) -> Result<Value, String> {
    if labels.is_empty() {
        return Err("chart: radar requires 'labels'".to_string());
    }
    let mut max = 0.0_f32;
    for dataset in datasets {
        if let Some(values) = dataset.get("data").and_then(Value::as_array) {
            for value in values {
                if let Some(value) = number(value) {
                    max = max.max(value);
                }
            }
        }
    }
    let limit = if max <= 0.0 { 1.0 } else { max * 1.1 };
    let indicators: Vec<Value> = labels
        .iter()
        .map(|label| json!({ "name": label, "max": limit }))
        .collect();
    let mut series = Vec::new();
    for (index, dataset) in datasets.iter().enumerate() {
        let data: Vec<f32> = dataset
            .get("data")
            .and_then(Value::as_array)
            .map(|values| values.iter().map(|value| number(value).unwrap_or(0.0)).collect())
            .unwrap_or_default();
        series.push(json!({
            "name": dataset.get("label").and_then(Value::as_str).unwrap_or(&format!("Series {}", index + 1)),
            "data": data
        }));
    }
    let mut chart = Map::new();
    chart.insert("width".to_string(), json!(1000));
    chart.insert("height".to_string(), json!(900));
    chart.insert("font_family".to_string(), json!("Roboto"));
    chart.insert("indicators".to_string(), Value::Array(indicators));
    chart.insert("series_list".to_string(), Value::Array(series));
    if !title.is_empty() {
        chart.insert("title_text".to_string(), json!(title));
        chart.insert("title_align".to_string(), json!("center"));
    }
    Ok(Value::Object(chart))
}

