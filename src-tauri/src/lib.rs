use std::env;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const DEFAULT_JIJINHAO_API_URL: &str = "https://api.jijinhao.com/quoteCenter/realTime.htm";
const DEFAULT_JIJINHAO_REFERER: &str = "https://quote.cngold.org/gjs/yhzhj.html";
const REFRESH_MS: u64 = 5000;

struct BankDef {
    id: &'static str,
    label: &'static str,
    cny_code: &'static str,
    usd_code: &'static str,
}

const BANKS: [BankDef; 7] = [
    BankDef {
        id: "ICBC",
        label: "工行",
        cny_code: "JO_42760",
        usd_code: "JO_42757",
    },
    BankDef {
        id: "CCB",
        label: "建行",
        cny_code: "JO_62286",
        usd_code: "JO_62288",
    },
    BankDef {
        id: "BOC",
        label: "中行",
        cny_code: "JO_283982",
        usd_code: "JO_283981",
    },
    BankDef {
        id: "ABC",
        label: "农行",
        cny_code: "JO_283972",
        usd_code: "JO_283974",
    },
    BankDef {
        id: "CIB",
        label: "兴业",
        cny_code: "JO_283982",
        usd_code: "JO_283981",
    },
    BankDef {
        id: "CMB",
        label: "招商",
        cny_code: "JO_283982",
        usd_code: "JO_283981",
    },
    BankDef {
        id: "JDMS",
        label: "京东民生",
        cny_code: "JO_283982",
        usd_code: "JO_283981",
    },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BankOption {
    id: String,
    label: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WidgetConfig {
    refresh_ms: u64,
    banks: Vec<BankOption>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Quote {
    value: Option<f64>,
    digits: Option<u32>,
    unit: String,
    show_name: String,
    quoted_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BankSnapshot {
    bank_id: String,
    bank_label: String,
    cny: Quote,
    usd: Quote,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    refreshed_at: String,
    data: Vec<BankSnapshot>,
}

#[tauri::command]
fn get_config() -> WidgetConfig {
    WidgetConfig {
        refresh_ms: REFRESH_MS,
        banks: BANKS
            .iter()
            .map(|bank| BankOption {
                id: bank.id.to_string(),
                label: bank.label.to_string(),
            })
            .collect(),
    }
}

#[tauri::command]
async fn get_latest() -> Result<Snapshot, String> {
    fetch_widget_snapshot().await
}

#[tauri::command]
fn hide_main_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn debug_log(scope: String, message: String) {
    println!("[frontend:{scope}] {message}");
}

fn get_jijinhao_api_url() -> String {
    env::var("JIJINHAO_API_URL").unwrap_or_else(|_| DEFAULT_JIJINHAO_API_URL.to_string())
}

fn get_jijinhao_referer() -> String {
    env::var("JIJINHAO_REFERER").unwrap_or_else(|_| DEFAULT_JIJINHAO_REFERER.to_string())
}

fn parse_jijinhao_payload(raw: &str) -> Result<Value, String> {
    let start = raw.find('{').ok_or_else(|| "行情响应格式异常".to_string())?;
    let end = raw.rfind('}').ok_or_else(|| "行情响应格式异常".to_string())?;
    if end <= start {
        return Err("行情响应格式异常".to_string());
    }
    serde_json::from_str(&raw[start..=end]).map_err(|e| format!("行情解析失败: {e}"))
}

fn to_number(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(num)) => num.as_f64(),
        Some(Value::String(s)) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn to_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Number(num)) => num.as_i64(),
        Some(Value::String(s)) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn to_u32(value: Option<&Value>) -> Option<u32> {
    match value {
        Some(Value::Number(num)) => num.as_u64().and_then(|v| u32::try_from(v).ok()),
        Some(Value::String(s)) => s.parse::<u32>().ok(),
        _ => None,
    }
}

fn to_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToString::to_string)
}

fn ms_to_iso(ms: Option<i64>) -> Option<String> {
    ms.and_then(DateTime::from_timestamp_millis)
        .map(|dt: DateTime<Utc>| dt.to_rfc3339())
}

fn cny_offset_from_boc(bank_id: &str) -> f64 {
    match bank_id {
        "ICBC" | "CCB" | "ABC" | "JDMS" => 4.0,
        "CIB" => 5.0,
        "CMB" => 6.0,
        _ => 0.0,
    }
}

fn is_derived_from_boc(bank_id: &str) -> bool {
    matches!(bank_id, "ICBC" | "CCB" | "ABC" | "CIB" | "CMB" | "JDMS")
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn build_snapshot_row(bank: &BankDef, payload: &Value) -> BankSnapshot {
    let empty = Value::Null;
    let cny = payload.get(bank.cny_code).unwrap_or(&empty);
    let usd = payload.get(bank.usd_code).unwrap_or(&empty);
    let mut cny_value = to_number(cny.get("q63"));
    if let Some(base_cny_value) = cny_value {
        cny_value = Some(base_cny_value + cny_offset_from_boc(bank.id));
    }

    BankSnapshot {
        bank_id: bank.id.to_string(),
        bank_label: bank.label.to_string(),
        cny: Quote {
            value: cny_value,
            digits: to_u32(cny.get("digits")),
            unit: to_string(cny.get("unit")).unwrap_or_else(|| "元/克".to_string()),
            show_name: if is_derived_from_boc(bank.id) {
                format!("{}纸黄金(人民币)", bank.label)
            } else {
                to_string(cny.get("showName"))
                    .unwrap_or_else(|| format!("{}纸黄金(人民币)", bank.label))
            },
            quoted_at: ms_to_iso(to_i64(cny.get("time"))),
        },
        usd: Quote {
            value: to_number(usd.get("q63")),
            digits: to_u32(usd.get("digits")),
            unit: to_string(usd.get("unit")).unwrap_or_else(|| "美元/盎司".to_string()),
            show_name: if is_derived_from_boc(bank.id) {
                format!("{}纸黄金(美元)", bank.label)
            } else {
                to_string(usd.get("showName"))
                    .unwrap_or_else(|| format!("{}纸黄金(美元)", bank.label))
            },
            quoted_at: ms_to_iso(to_i64(usd.get("time"))),
        },
    }
}

async fn fetch_widget_snapshot() -> Result<Snapshot, String> {
    let codes = BANKS
        .iter()
        .flat_map(|bank| [bank.cny_code, bank.usd_code])
        .collect::<Vec<_>>()
        .join(",");

    let mut url =
        reqwest::Url::parse(&get_jijinhao_api_url()).map_err(|e| format!("行情地址无效: {e}"))?;
    url.query_pairs_mut().append_pair("codes", &codes).append_pair(
        "_",
        &Utc::now().timestamp_millis().to_string(),
    );

    let response = reqwest::Client::new()
        .get(url)
        .header("accept", "*/*")
        .header("referer", get_jijinhao_referer())
        .header("accept-language", "zh-CN,zh;q=0.9")
        .send()
        .await
        .map_err(|e| format!("请求行情接口失败: {e}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取行情接口失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("行情接口失败: {status}"));
    }

    let payload = parse_jijinhao_payload(&body)?;

    Ok(Snapshot {
        refreshed_at: Utc::now().to_rfc3339(),
        data: BANKS
            .iter()
            .map(|bank| build_snapshot_row(bank, &payload))
            .collect(),
    })
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItemBuilder::new("显示窗口").id("show").build(app)?;
    let quit_item = MenuItemBuilder::new("退出").id("quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show_item, &quit_item])
        .build()?;

    let mut builder = TrayIconBuilder::with_id("gold-tray")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            match event {
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => {
                    show_main_window(tray.app_handle());
                }
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    #[cfg(not(target_os = "windows"))]
                    show_main_window(tray.app_handle());
                }
                _ => {}
            }
        });

    if let Ok(icon) = Image::from_bytes(include_bytes!("../icons/32x32.png")) {
        builder = builder.icon(icon);
    } else if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_latest,
            hide_main_window,
            exit_app,
            debug_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
