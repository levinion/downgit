use std::{
    fs::{create_dir_all, File},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Result};
use tokio::sync::broadcast::{channel, Sender};

#[derive(serde::Deserialize, Debug)]
struct Node {
    path: String,
    size: Option<isize>,
}

#[derive(serde::Deserialize, Debug)]
struct FileTree {
    tree: Vec<Node>,
}

#[derive(Clone, Copy, Debug)]
pub struct Process {
    pub current: usize,
    pub all: usize,
}

impl Process {
    fn new(n: usize) -> Arc<Mutex<Self>> {
        let this = Self { current: 0, all: n };
        Arc::new(Mutex::new(this))
    }

    fn deep_clone(&self) -> Self {
        Self {
            current: self.current,
            all: self.all,
        }
    }

    fn done(&mut self) {
        self.current += 1;
    }

    pub fn percent(&self) -> f64 {
        self.current as f64 / self.all as f64
    }

    pub fn is_over(&self) -> bool {
        self.current == self.all
    }
}

macro_rules! send_if_err {
    ($tx: expr,$result: expr) => {
        if let Err(err) = $result {
            $tx.send(Err(err.to_string())).unwrap();
            return;
        }
        $result.unwrap()
    };
}

impl FileTree {
    async fn download(&self, downloader: Arc<Downloader>, tx: Sender<Result<Process, String>>) {
        let tasks: Vec<_> = self
            .tree
            .iter()
            .filter(|node| node.size.is_some())
            .map(|node| Arc::new(PathBuf::from(&node.path)))
            .filter(|path| {
                let src = PathBuf::from(&downloader.remote_path);
                path.starts_with(src)
            })
            .collect();
        let process = Process::new(tasks.len());
        tasks.iter().for_each(|path| {
            let src = PathBuf::from(&downloader.remote_path);
            let dst = PathBuf::from(&downloader.local_path);
            let path = path.clone();
            let tx = tx.clone();
            let downloader = downloader.clone();
            let process = process.clone();
            tokio::spawn(async move {
                // src is remote path, such as nvim/init.lua
                // dst is local path such as src
                // path is the exact remote path, on the situation of single file, path equals with src
                // the final dst is the download path, such as src/init.lua
                let dst = dst.join(path.strip_prefix(&src).unwrap());
                let dst_dir = dst.parent().unwrap();
                create_dir_all(dst_dir).unwrap();
                send_if_err!(
                    tx,
                    downloader
                        .download_single(
                            path.as_os_str().to_str().unwrap(),
                            dst.to_str().unwrap().trim_end_matches("/")
                        )
                        .await
                );
                let mut lock = process.lock().unwrap();
                lock.done();
                let process = lock.deep_clone();
                tx.send(Ok(process)).unwrap();
            });
        });
    }
}

pub struct Downloader {
    user: String,
    repo: String,
    branch: String,
    remote_path: String,
    local_path: String,
    process_handler: fn(Process),
}

impl Downloader {
    const USER_AGENT:&'static str="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.5410.0 Safari/537.36";

    async fn download_single(&self, path: &str, dst: &str) -> Result<()> {
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{path}",
            &self.user, &self.repo, &self.branch
        );
        let client = reqwest::ClientBuilder::new()
            .user_agent(Self::USER_AGENT)
            .build()?;
        let res = client.get(url).send().await?.text().await?;
        let mut file = File::create(dst)?;
        file.write_all(res.as_bytes())?;
        Ok(())
    }

    pub async fn download(self) -> Result<()> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            &self.user, &self.repo, &self.branch
        );
        let client = reqwest::ClientBuilder::new()
            .user_agent(Self::USER_AGENT)
            .build()?;
        let res = client.get(url).send().await.unwrap().text().await.unwrap();
        let file_tree: FileTree = serde_json::from_str(&res)
            .map_err(|_| anyhow!("Are you sure the repo really exists?"))?;

        let (tx, mut rx) = channel::<Result<Process, String>>(5);

        let me = Arc::new(self);

        file_tree.download(me.clone(), tx).await;

        loop {
            let process = rx
                .recv()
                .await
                .map_err(|_| anyhow!("Are you sure the target name is right?"))?
                .unwrap();
            (me.process_handler)(process);
            if process.is_over() {
                return Ok(());
            }
        }
    }
}

#[derive(Default)]
pub struct DownloaderBuilder {
    user: String,
    repo: String,
    branch: Option<String>,
    remote_path: String,
    local_path: Option<String>,
    process_handler: Option<fn(Process)>,
}

impl DownloaderBuilder {
    pub fn new(user: &str, repo: &str, remote: &str) -> Self {
        Self {
            user: user.into(),
            repo: repo.into(),
            remote_path: remote.into(),
            ..Default::default()
        }
    }

    pub fn branch(mut self, branch: &str) -> Self {
        self.branch = Some(branch.into());
        self
    }

    pub fn local_path(mut self, local: &str) -> Self {
        let remote = PathBuf::from(&self.remote_path);
        let name = remote.file_name().unwrap().to_str().unwrap().to_string();
        let local = PathBuf::from(local);
        self.local_path = Some(local.join(name).to_str().unwrap().to_string());
        self
    }

    pub fn on_process(mut self, f: fn(Process)) -> Self {
        self.process_handler = Some(f);
        self
    }

    pub fn build(self) -> Downloader {
        let path = PathBuf::from(&self.remote_path);
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        Downloader {
            user: self.user,
            repo: self.repo,
            remote_path: self.remote_path,
            branch: self.branch.unwrap_or("main".into()),
            local_path: self.local_path.unwrap_or(name),
            process_handler: self.process_handler.unwrap_or(|_| {}),
        }
    }
}
