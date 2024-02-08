use std::collections::HashMap;
use std::io::{Read, Write};
use reqwest;
use reqwest::header;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use scraper::{Html, Selector};
use rand::Rng;
use tokio;
use tokio::time::Duration;

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct CheckInStatusResponse {
    no: u16,
    error: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
enum CheckInStatus {
    #[serde(rename = "0")]
    Success,
    #[serde(rename = "1101")]
    AlreadyCheckedIn,
    #[serde(rename = "1102")]
    TooFast,
    #[serde(other)]
    UnknownError,
}

fn parse_check_in_status(status: u16) -> CheckInStatus {
    match status {
        0 => CheckInStatus::Success,
        1101 => CheckInStatus::AlreadyCheckedIn,
        1102 => CheckInStatus::TooFast,
        _ => CheckInStatus::UnknownError,
    }
}

fn read_cookie() -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open("cookie.txt")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

fn build_header(cookie: String) -> Result<HeaderMap, reqwest::Error> {
    let mut headers = header::HeaderMap::new();
    headers.insert("Accept", "application/json, text/javascript, */*; q=0.01".parse().unwrap());
    headers.insert("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8,en-GB;q=0.7,en-US;q=0.6".parse().unwrap());
    headers.insert("Connection", "keep-alive".parse().unwrap());
    headers.insert("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8".parse().unwrap());
    headers.insert(header::COOKIE, cookie.parse().unwrap());
    headers.insert("Origin", "https://tieba.baidu.com".parse().unwrap());
    // headers.insert("Referer", "https://tieba.baidu.com/f?kw=%E6%88%98%E4%BA%89%E9%9B%B7%E9%9C%86&fr=index".parse().unwrap());
    headers.insert("Sec-Fetch-Dest", "empty".parse().unwrap());
    headers.insert("Sec-Fetch-Mode", "cors".parse().unwrap());
    headers.insert("Sec-Fetch-Site", "same-origin".parse().unwrap());
    headers.insert("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36 Edg/121.0.0.0".parse().unwrap());
    headers.insert("X-Requested-With", "XMLHttpRequest".parse().unwrap());
    headers.insert("sec-ch-ua", "\"Not A(Brand\";v=\"99\", \"Microsoft Edge\";v=\"121\", \"Chromium\";v=\"121\"".parse().unwrap());
    headers.insert("sec-ch-ua-mobile", "?0".parse().unwrap());
    headers.insert("sec-ch-ua-platform", "\"Windows\"".parse().unwrap());
    Ok(headers)
}

async fn user_id(cookie: String) -> Result<String, reqwest::Error> {
    let headers = build_header(cookie)?;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client.get("https://tieba.baidu.com/mo/q/sync")
        .headers(headers)
        .send()
        .await?;

    let res_text = res.text().await?;
    let res_json: serde_json::Value = serde_json::from_str(&res_text).unwrap();

    if res_json.clone()["no"] != 0 {
        println!(">> 获取用户ID失败！");
        panic!("获取用户ID失败！")
    }
    println!(">> 获取用户ID成功！");
    let user_id = res_json["data"]["user_id"].as_u64().unwrap();
    println!(">> 用户ID: {}", user_id);
    Ok(user_id.to_string())
}


async fn all_subscribed_tieba(cookie: String) -> Result<Vec<String>, reqwest::Error> {
    let headers = build_header(cookie)?;
    let mut all_tieba: Vec<String> = Vec::new();

    let mut pn_index: u16 = 1;

    loop {
        let mut params = HashMap::new();
        params.insert("pn", pn_index);

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let res = client.get("https://tieba.baidu.com/f/like/mylike")
            .headers(headers.clone())
            .query(&params)
            .send()
            .await?;

        let res_html = Html::parse_document(&res.text().await?);

        let table_selector = Selector::parse("table").unwrap();
        let table = match res_html.select(&table_selector).next() {
            Some(table) => table,
            None => break, // 如果没有找到表格，可能是最后一页或出错了
        };

        // 判断table内容的二进制数据长度是否小于200，如果是则说明已经到了最后一页
        if table.inner_html().as_bytes().len() < 200 {
            break;
        }

        let tr_selector = Selector::parse("tr").unwrap();
        for tr in table.select(&tr_selector) {
            let td_selector = Selector::parse("td").unwrap();
            let td: Vec<_> = tr.select(&td_selector).collect();
            if td.len() >= 2 {
                let a_selector = Selector::parse("a").unwrap();
                if let Some(a) = td[0].select(&a_selector).next() {
                    let a_text = a.text().collect::<Vec<_>>().join("");
                    if all_tieba.contains(&a_text) {
                        break;
                    }
                    all_tieba.push(a_text.clone());
                    // println!("{}", a_text);
                }
            }
        }

        println!(">> 正在获取已关注的贴吧列表，当前页数: {}", pn_index);
        pn_index += 1;

        let mut rng = rand::thread_rng();
        let sleep_time = rng.gen_range(0..3);
        tokio::time::sleep(Duration::from_secs(sleep_time)).await;
    }
    println!(">> 获取已关注的贴吧列表成功, 共关注 {} 个贴吧!", all_tieba.len());

    Ok(all_tieba)
}

async fn check_in(cookie: String, subscribe: Vec<String>) -> Result<(), reqwest::Error> {
    let headers = build_header(cookie)?;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    for tieba in subscribe {
        println!(">> 正在签到: {} ...", tieba);
        let res = client.post("https://tieba.baidu.com/sign/add")
            .headers(headers.clone())
            .body("ie=utf-8&kw=".to_owned() + &tieba)
            .send()
            .await?;

        // println!("{}", res.status());

        // 先获取响应文本，再进行操作
        let res_text = res.text().await?;

        // 使用之前获取的响应文本
        let check_in_status_json: CheckInStatusResponse = serde_json::from_str(&res_text).unwrap();
        // println!("{:?}", check_in_status_json);

        let status = parse_check_in_status(check_in_status_json.no);
        match status {
            CheckInStatus::Success => println!(">> {} 签到成功!", tieba),
            CheckInStatus::AlreadyCheckedIn => println!(">> {} 今天已经签到过了!", tieba),
            CheckInStatus::TooFast => println!(">> 签到的太快了!"),
            CheckInStatus::UnknownError => println!(">> {} 签到时发生未知错误!", tieba),
        }

        let mut rng = rand::thread_rng();
        let sleep_time = rng.gen_range(3..5);
        tokio::time::sleep(Duration::from_secs(sleep_time)).await;
    }

    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    println!("══════════════════════════════════════════════════════════════════════");
    println!("                  百度贴吧自动签到程序 v1.0");
    println!("                      作者: YenHarvey");
    println!("                      博客: https://www.mewpaz.com");
    println!("              项目地址: https://github.com/YenHarvey/BaiduTiebaCheckIn.git");
    println!("══════════════════════════════════════════════════════════════════════");
    println!("【免责声明】本程序仅供学习交流使用，请勿用于任何商业用途！");
    println!("══════════════════════════════════════════════════════════════════════");
    println!("【程序描述】");
    println!("本程序用于自动签到百度贴吧。它会自动遍历并签到用户已关注的贴吧列表。");
    println!("为确保程序的正常运行，请遵循以下指南：");
    println!("  - 确保您已经登录百度账号，并且关注了一些贴吧。");
    println!("  - 请确保 cookie.txt 文件存在，内容正确，并位于程序运行的同级目录下。");
    println!("  - 程序运行时，请保持网络连接正常。");
    println!("  - 为防止账户被百度封禁，每次签到后程序会随机等待 3-5 秒。");
    println!("══════════════════════════════════════════════════════════════════════");


    match read_cookie() {
        Ok(cookie) => {
            println!(">> 读取cookie成功!");
            user_id(cookie.clone()).await?;
            check_in(cookie.clone(), all_subscribed_tieba(cookie).await?).await?;
        }
        Err(e) => {
            println!(">> 读取cookie失败: {}", e);
            println!(">> 请确保cookie.txt文件存在，内容正确，并且位于程序运行同级目录下!");
        }
    }

    // 防止闪屏
    print!("按任意键退出...");
    let _ = std::io::stdout().flush();
    let _ = std::io::stdin().read(&mut [0u8]).unwrap();

    Ok(())
}
