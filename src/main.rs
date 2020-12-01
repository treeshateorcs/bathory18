use chrono::TimeZone;
use std::io::BufRead;
use std::io::Read;
use std::io::Write;

const TIMEOUT: u64 = 60;
const LAST_ARTICLES: usize = 100; // this many articles to display

#[derive(PartialEq)]
struct Item {
  read: i8,
  timestamp: chrono::DateTime<chrono::Utc>,
  feed_title: String,
  command: String,
  item: feed_rs::model::Entry,
}

#[derive(serde::Deserialize)]
struct ReadItem {
  date: i64,
  link: String,
}

fn main() {
  let xdg_dirs = xdg::BaseDirectories::new().unwrap();
  let lock = xdg_dirs.place_config_file("bathory18/lock").unwrap();
  let instance_a = single_instance::SingleInstance::new(lock.to_str().unwrap()).unwrap();
  if !instance_a.is_single() {
    println!("only one instance of swkb at a time is allowed");
    std::process::exit(1);
  }
  let urls = xdg_dirs.place_config_file("bathory18/urls").unwrap();
  let urls = std::fs::File::open(&urls).expect(
    format!(
      "create file {} and add your feeds in it",
      urls.to_str().unwrap()
    )
    .as_str(),
  );
  let mut reader = std::io::BufReader::new(urls);
  let mut lines = Vec::new();
  for line in reader.by_ref().lines() {
    let line2 = line.unwrap().clone();
    if line2.trim().starts_with('#') || line2.trim().is_empty() {
      continue;
    }
    lines.push(line2);
  }
  let mut first_time: bool = true;
  let args: Vec<_> = std::env::args().collect();
  loop {
    let mut feeds: Vec<Item> = Vec::new();
    let feed_threads = parse_feeds(&lines);
    for feed_thread in feed_threads {
      let fee = feed_thread.join().unwrap_or(Vec::new());
      for f in fee {
        feeds.push(f);
      }
    }
    remove_read_items(&mut feeds);
    feeds.sort_unstable_by(|a, b| {
      if b.timestamp != a.timestamp {
        a.timestamp
          .timestamp()
          .partial_cmp(&b.timestamp.timestamp())
          .unwrap()
      } else if b.item.title != a.item.title {
        b.item
          .title
          .as_ref()
          .unwrap()
          .content
          .partial_cmp(&a.item.title.as_ref().unwrap().content)
          .unwrap()
      } else {
        b.feed_title.partial_cmp(&a.feed_title).unwrap()
      }
    });
    if first_time && args.len() > 1 && args[1].eq("-a") {
      mark_all_as_read(&mut feeds);
      first_time = false;
    }
    for article in feeds.iter().rev().take(LAST_ARTICLES) {
      let args = vec![article.item.links[0].href.clone()];
      notify_rust::Notification::new()
        .summary(article.feed_title.as_str())
        .body(article.item.title.as_ref().unwrap().content.as_str())
        .timeout(notify_rust::Timeout::Never)
        .action("default", "default")
        .show()
        .unwrap()
        .wait_for_action(|action| match action {
          "default" => open_url(&article.command, &args),
          _ => (),
        });
      mark_as_read(article);
    }
    std::thread::sleep(std::time::Duration::from_secs(TIMEOUT));
  }
}

fn mark_all_as_read(entries: &mut Vec<Item>) {
  let xdg_dirs = xdg::BaseDirectories::new().unwrap();
  let read = xdg_dirs.place_config_file("bathory18/read").unwrap();
  let mut file = std::fs::OpenOptions::new()
    .read(true)
    .append(true)
    .create(true)
    .open(read)
    .unwrap();
  let mut write = String::new();
  for entry in entries {
    if entry.read == 0 {
      entry.read = 1;
      write.push_str(&format!(
        "{},{}\n",
        entry
          .item
          .published
          .unwrap_or_else(|| chrono::Utc.timestamp(0, 0))
          .timestamp(),
        entry.item.links[0].href
      ))
    }
  }
  file.write(write.as_bytes()).ok();
}

fn parse_feeds(lines: &Vec<String>) -> Vec<std::thread::JoinHandle<Vec<Item>>> {
  let mut feed_threads = Vec::new();
  for line in lines.clone() {
    feed_threads.push(std::thread::spawn(move || -> Vec<Item> {
      let mut url_title_command_vec: Vec<_> = line.splitn(3, ",").collect();
      while url_title_command_vec.len() < 3 {
        url_title_command_vec.push("");
      }
      let (url, mut title, mut command) = (
        String::from(url_title_command_vec[0]),
        String::from(url_title_command_vec[1]),
        String::from(url_title_command_vec[2]),
      );
      if command.is_empty() {
        command = String::from("xdg-open");
      }
      let mut feeds = Vec::new();
      let res = match reqwest::blocking::get(url.as_str()) {
        Ok(r) => r.text(),
        Err(_) => {
          return feeds;
        }
      };
      let feed = match feed_rs::parser::parse(res.unwrap_or_default().as_bytes()) {
        Ok(f) => f,
        Err(_) => {
          return feeds;
        }
      };
      if title.len() == 0 {
        title = match feed.title {
          Some(t) => t.content,
          None => String::new(),
        };
      }
      for mut entry in feed.entries {
        if entry.links.is_empty() {
          entry.links.push(feed_rs::model::Link {
            href: "http://example.com".to_string(),
            href_lang: Some(String::new()),
            length: Some(0),
            media_type: Some(String::new()),
            rel: Some(String::new()),
            title: Some(String::new()),
          });
        }
        let i = Item {
          read: 0,
          feed_title: title.clone(),
          command: command.clone(),
          timestamp: match entry.published {
            Some(t) => t,
            None => match entry.updated {
              Some(t) => t,
              None => chrono::DateTime::<chrono::Utc>::from_utc(
                chrono::NaiveDateTime::from_timestamp(1, 0),
                chrono::Utc,
              ),
            },
          },
          item: entry,
        };
        feeds.push(i);
      }
      feeds
    }));
  }
  return feed_threads;
}

fn remove_read_items(entries: &mut Vec<Item>) {
  let xdg_dirs = xdg::BaseDirectories::new().unwrap();
  let read = xdg_dirs.place_config_file("bathory18/read").unwrap();
  let file = std::fs::read_to_string(read).unwrap_or_else(|_| "0,\"\"".to_string());
  let mut rdr = csv::ReaderBuilder::new()
    .has_headers(false)
    .from_reader(file.as_bytes());
  let mut read_items = Vec::new();
  for result in rdr.records() {
    let record = result.unwrap();
    let row: ReadItem = record.deserialize(None).unwrap();
    read_items.push(row);
  }
  for row in read_items {
    entries.retain(|entry| {
      !(entry
        .item
        .published
        .unwrap_or_else(|| chrono::Utc.timestamp(0, 0))
        .timestamp()
        == row.date
        && entry.item.links[0].href == row.link)
    });
  }
}

fn open_url(command: &String, args: &Vec<String>) {
  std::process::Command::new(command)
    .args(args)
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .spawn()
    .ok()
    .expect("failed to spawn process");
}

fn mark_as_read(entry: &Item) {
  let xdg_dirs = xdg::BaseDirectories::new().unwrap();
  let read = xdg_dirs.place_config_file("bathory18/read").unwrap();
  let mut file = std::fs::OpenOptions::new()
    .read(true)
    .append(true)
    .create(true)
    .open(read)
    .unwrap();
  let mut rdr = csv::ReaderBuilder::new()
    .has_headers(false)
    .from_reader(&file);
  for result in rdr.records() {
    let record = result.unwrap();
    let row: ReadItem = record.deserialize(None).unwrap();
    if row.date
      == entry
        .item
        .published
        .unwrap_or_else(|| chrono::Utc.timestamp(0, 0))
        .timestamp()
      && entry.item.links[0].href == row.link
    {
      return;
    }
  }
  file
    .write(
      format!(
        "{},{}\n",
        entry
          .item
          .published
          .unwrap_or_else(|| chrono::Utc.timestamp(0, 0))
          .timestamp(),
        entry.item.links[0].href
      )
      .as_bytes(),
    )
    .ok();
}
