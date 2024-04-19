use anyhow::Result;
use gitload::DownloaderBuilder;

#[tokio::main]
async fn main() -> Result<()> {
    let downloader = DownloaderBuilder::new("levinion", "dotfiles", "nvim")
        .on_process(|process| {
            println!(
                "process: {}/{}\t{:.0}%",
                process.current,
                process.all,
                process.percent() * 100.
            );
        })
        .build();
    downloader.download().await
}
