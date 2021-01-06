use std::io::BufRead;
use std::io::Read;
use std::io::Write;

#[derive(PartialEq)]
struct Item {
  read: i8,
  timestamp: i64,
  feed_title: String,
  command: String,
  article_title: String,
  link: String,
}

#[derive(serde::Deserialize)]
struct ReadItem {
  date: i64,
  link: String,
}

fn main() {
  let timeout = std::env::var("BATHORY18_TIMEOUT")
    .unwrap_or(String::from("60"))
    .parse::<u64>()
    .unwrap_or(60);
  println!("timeout set to {}", timeout);
  let config_dir = dirs::config_dir().unwrap();
  let mut lock = config_dir.clone();
  lock.push("bathory18/lock");
  let instance_a = single_instance::SingleInstance::new(lock.to_str().unwrap()).unwrap();
  if !instance_a.is_single() {
    println!(
      "only one instance of {:?} at a time is allowed",
      std::env::args().take(1)
    );
    std::process::exit(1);
  }
  let mut urls = config_dir;
  urls.push("bathory18/urls");
  let urls = std::fs::File::open(&urls).unwrap_or_else(|_| {
    panic!(
      "create file {} and add your feeds in it",
      urls.to_str().unwrap()
    )
  });
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
      let fee = feed_thread.join().unwrap_or_default();
      for f in fee {
        feeds.push(f);
      }
    }
    remove_read_items(&mut feeds);
    feeds.sort_unstable_by(|b, a| {
      if b.timestamp != a.timestamp {
        b.timestamp.partial_cmp(&a.timestamp).unwrap()
      } else if b.article_title != a.article_title {
        b.article_title.partial_cmp(&a.article_title).unwrap()
      } else {
        b.feed_title.partial_cmp(&a.feed_title).unwrap()
      }
    });
    if first_time && args.len() > 1 && args[1].eq("-a") {
      mark_all_as_read(&mut feeds);
      first_time = false;
      continue;
    }
    let len = feeds.len();
    let mut index = 1;
    for article in feeds.iter().rev() {
      notify(&article, &index, &len);
      mark_as_read(article);
      index += 1;
    }
    std::thread::sleep(std::time::Duration::from_secs(timeout));
  }
}

#[cfg(target_os = "windows")]
fn notify(article: &Item) {
  notify_rust::Notification::new()
    .summary(article.feed_title.as_str())
    .body(&article.article_title)
    .timeout(notify_rust::Timeout::Never)
    .action("default", "default")
    .show()
    .unwrap();
}

#[cfg(not(target_os = "windows"))]
fn notify(article: &Item, index: &i32, len: &usize) {
  let args = vec![&article.link];
  let title = if *len == 1 {
    format!("{}", article.feed_title)
  } else {
    format!("({}/{}) {}", index, len, article.feed_title)
  };
  notify_rust::Notification::new()
    .summary(title.as_str())
    .body(&article.article_title)
    .timeout(notify_rust::Timeout::Never)
    .action("default", "default")
    .show()
    .unwrap()
    .wait_for_action(|action| {
      if let "default" = action {
        open_url(&article.command, &args);
      }
    });
}

fn mark_all_as_read(entries: &mut Vec<Item>) {
  let mut read = dirs::config_dir().unwrap();
  read.push("bathory18/read");
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
      write.push_str(&format!("{},{}\n", entry.timestamp, entry.link))
    }
  }
  file.write(write.as_bytes()).ok();
}

fn parse_feeds(lines: &Vec<String>) -> Vec<std::thread::JoinHandle<Vec<Item>>> {
  let mut feed_threads = Vec::new();
  for line in lines.clone() {
    feed_threads.push(std::thread::spawn(move || -> Vec<Item> {
      let mut url_title_command_vec: Vec<_> = line.splitn(3, ',').collect();
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
      if title.is_empty() {
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
          article_title: entry.title.unwrap().content,
          timestamp: match entry.published {
            Some(t) => t.timestamp(),
            None => match entry.updated {
              Some(t) => t.timestamp(),
              None => 0,
            },
          },
          link: entry.links[0].href.clone(),
        };
        feeds.push(i);
      }
      feeds
    }));
  }
  feed_threads
}

fn remove_read_items(entries: &mut Vec<Item>) {
  let mut read = dirs::config_dir().unwrap();
  read.push("bathory18/read");
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
    entries.retain(|entry| !(entry.timestamp == row.date && entry.link == row.link));
  }
}
#[cfg(not(target_os = "windows"))]
fn open_url(command: &str, args: &Vec<&String>) {
  std::process::Command::new(command)
    .args(args)
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .spawn()
    .expect("failed to spawn process");
}

fn mark_as_read(entry: &Item) {
  let mut read = dirs::config_dir().unwrap();
  read.push("bathory18/read");
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
    if row.date == entry.timestamp && entry.link == row.link {
      return;
    }
  }
  file
    .write(format!("{},{}\n", entry.timestamp, entry.link).as_bytes())
    .ok();
}
