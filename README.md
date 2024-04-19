# gitdown

Gitdown is a Rust library for downloading specific folders from GitHub repositories.

## usage

```rust
use anyhow::Result;
use gitload::DownloaderBuilder;

#[tokio::main]
async fn main() -> Result<()> {
    let downloader = DownloaderBuilder::new("<user>", "<repo>", "<directory>")
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
```
