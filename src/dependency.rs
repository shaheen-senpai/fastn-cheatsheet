pub trait DependencyProvider {
    fn download(&self) -> bool;
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
    pub repo: String,
    pub notes: Option<String>,
}

impl Dependency {
    pub async fn process(&self, base_dir: camino::Utf8PathBuf) -> fpm::Result<()> {
        use tokio::io::AsyncWriteExt;
        if base_dir.join(".packages").join(self.name.as_str()).exists() {
            return Ok(());
        }

        let download_url = match self.repo.as_str() {
            "github" => {
                format!(
                    "https://github.com/{}/archive/refs/heads/main.zip",
                    self.name
                )
            }
            k => k.to_string(),
        };

        let response = reqwest::get(download_url).await?;

        let download_path = format!("/tmp/{}.zip", self.name.replace("/", "__"));
        let path = std::path::Path::new(download_path.as_str());

        {
            let mut file = tokio::fs::File::create(&path).await?;
            let content = response.bytes().await?;
            file.write_all(&content).await?;
        }

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for i in 0..archive.len() {
            let mut c_file = archive.by_index(i).unwrap();
            let out_path = match c_file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };
            let out_path_without_folder = out_path.to_str().unwrap().split_once("/").unwrap().1;
            let new_out_path = format!("{}/{}", self.name, out_path_without_folder);
            let file_extract_path = std::path::Path::new(new_out_path.as_str());
            if (&*c_file.name()).ends_with('/') {
                std::fs::create_dir_all(
                    format!("./.packages/{}", file_extract_path.to_str().unwrap()).as_str(),
                )?;
            } else {
                if let Some(p) = file_extract_path.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(
                            format!("./.packages/{}", &p.to_str().expect("")).as_str(),
                        )?;
                    }
                }
                let mut outfile = std::fs::File::create(format!(
                    "./.packages/{}",
                    &file_extract_path.to_str().expect("")
                ))?;
                std::io::copy(&mut c_file, &mut outfile)?;
            }
        }
        Ok(())
    }
}

pub async fn ensure(base_dir: camino::Utf8PathBuf, deps: Vec<fpm::Dependency>) -> fpm::Result<()> {
    futures::future::join_all(
        deps.into_iter()
            .map(|x| (x, base_dir.clone()))
            .map(|(x, base_dir)| tokio::spawn(async move { x.process(base_dir).await }))
            .collect::<Vec<tokio::task::JoinHandle<_>>>(),
    )
    .await;
    Ok(())
}
