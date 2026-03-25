use std::path::Path;

use crate::cli::config_parser::AppType;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ProjectDetection — result of scanning a project directory
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone)]
pub struct ProjectDetection {
    pub app_type: AppType,
    pub has_dockerfile: bool,
    pub has_compose: bool,
    pub has_env_file: bool,
    pub detected_files: Vec<String>,
}

impl Default for ProjectDetection {
    fn default() -> Self {
        Self {
            app_type: AppType::Custom,
            has_dockerfile: false,
            has_compose: false,
            has_env_file: false,
            detected_files: Vec::new(),
        }
    }
}

/// Convert a detection result into the detected AppType.
impl From<&ProjectDetection> for AppType {
    fn from(detection: &ProjectDetection) -> Self {
        detection.app_type
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// FileSystem trait — abstraction for testability (DIP)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub trait FileSystem: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
    fn list_dir(&self, path: &Path) -> Result<Vec<String>, std::io::Error>;
}

/// Production filesystem using std::fs.
pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<String>, std::io::Error> {
        let entries = std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        Ok(entries)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Detection markers — which files map to which app type
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

struct DetectionMarker {
    filename: &'static str,
    app_type: AppType,
    priority: u8, // higher = stronger signal
}

/// Ordered list of detection markers. Higher priority takes precedence.
const DETECTION_MARKERS: &[DetectionMarker] = &[
    DetectionMarker {
        filename: "Cargo.toml",
        app_type: AppType::Rust,
        priority: 10,
    },
    DetectionMarker {
        filename: "go.mod",
        app_type: AppType::Go,
        priority: 10,
    },
    DetectionMarker {
        filename: "composer.json",
        app_type: AppType::Php,
        priority: 10,
    },
    DetectionMarker {
        filename: "package.json",
        app_type: AppType::Node,
        priority: 9,
    },
    DetectionMarker {
        filename: "pyproject.toml",
        app_type: AppType::Python,
        priority: 9,
    },
    DetectionMarker {
        filename: "requirements.txt",
        app_type: AppType::Python,
        priority: 8,
    },
    DetectionMarker {
        filename: "index.html",
        app_type: AppType::Static,
        priority: 5,
    },
];

/// Infrastructure files to detect alongside app type.
const DOCKERFILE_NAMES: &[&str] = &["Dockerfile", "dockerfile"];
const COMPOSE_NAMES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];
const ENV_FILE_NAMES: &[&str] = &[".env"];

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// detect_project — scan a directory to identify project type
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Detect the project type and infrastructure files in a directory.
pub fn detect_project(
    project_path: &Path,
    fs: &dyn FileSystem,
) -> ProjectDetection {
    let files = match fs.list_dir(project_path) {
        Ok(f) => f,
        Err(_) => return ProjectDetection::default(),
    };

    let mut detection = ProjectDetection::default();
    let mut best_priority: u8 = 0;

    for filename in &files {
        // Check app type markers
        for marker in DETECTION_MARKERS {
            if filename == marker.filename && marker.priority > best_priority {
                detection.app_type = marker.app_type;
                best_priority = marker.priority;
                if !detection.detected_files.contains(filename) {
                    detection.detected_files.push(filename.clone());
                }
            }
        }

        // Check infrastructure files
        if DOCKERFILE_NAMES.iter().any(|n| n == filename) {
            detection.has_dockerfile = true;
            detection.detected_files.push(filename.clone());
        }

        if COMPOSE_NAMES.iter().any(|n| n == filename) {
            detection.has_compose = true;
            detection.detected_files.push(filename.clone());
        }

        if ENV_FILE_NAMES.iter().any(|n| n == filename) {
            detection.has_env_file = true;
        }
    }

    detection
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests — Phase 2
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory mock filesystem for deterministic testing without I/O.
    struct MockFileSystem {
        files: Vec<String>,
    }

    impl MockFileSystem {
        fn with_files(files: &[&str]) -> Self {
            Self {
                files: files.iter().map(|s| s.to_string()).collect(),
            }
        }
    }

    impl FileSystem for MockFileSystem {
        fn exists(&self, path: &Path) -> bool {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            self.files.contains(&name)
        }

        fn list_dir(&self, _path: &Path) -> Result<Vec<String>, std::io::Error> {
            Ok(self.files.clone())
        }
    }

    fn detect_with(files: &[&str]) -> ProjectDetection {
        let fs = MockFileSystem::with_files(files);
        detect_project(Path::new("/test"), &fs)
    }

    #[test]
    fn test_detect_static_html() {
        let det = detect_with(&["index.html", "style.css"]);
        assert_eq!(det.app_type, AppType::Static);
    }

    #[test]
    fn test_detect_node_project() {
        let det = detect_with(&["package.json", "src"]);
        assert_eq!(det.app_type, AppType::Node);
    }

    #[test]
    fn test_detect_python_requirements() {
        let det = detect_with(&["requirements.txt", "app.py"]);
        assert_eq!(det.app_type, AppType::Python);
    }

    #[test]
    fn test_detect_python_pyproject() {
        let det = detect_with(&["pyproject.toml"]);
        assert_eq!(det.app_type, AppType::Python);
    }

    #[test]
    fn test_detect_rust_project() {
        let det = detect_with(&["Cargo.toml", "src"]);
        assert_eq!(det.app_type, AppType::Rust);
    }

    #[test]
    fn test_detect_go_project() {
        let det = detect_with(&["go.mod", "main.go"]);
        assert_eq!(det.app_type, AppType::Go);
    }

    #[test]
    fn test_detect_php_composer() {
        let det = detect_with(&["composer.json", "public"]);
        assert_eq!(det.app_type, AppType::Php);
    }

    #[test]
    fn test_detect_empty_directory() {
        let det = detect_with(&[]);
        assert_eq!(det.app_type, AppType::Custom);
    }

    #[test]
    fn test_detect_priority_node_over_static() {
        let det = detect_with(&["package.json", "index.html"]);
        assert_eq!(
            det.app_type,
            AppType::Node,
            "package.json (priority 9) should beat index.html (priority 5)"
        );
    }

    #[test]
    fn test_detect_existing_dockerfile_flag() {
        let det = detect_with(&["Dockerfile", "package.json"]);
        assert!(det.has_dockerfile);
        assert_eq!(det.app_type, AppType::Node);
    }

    #[test]
    fn test_detect_existing_compose_flag() {
        let det = detect_with(&["docker-compose.yml", "index.html"]);
        assert!(det.has_compose);
    }

    #[test]
    fn test_detect_env_file_flag() {
        let det = detect_with(&[".env", "index.html"]);
        assert!(det.has_env_file);
    }

    #[test]
    fn test_detection_to_app_type_via_from() {
        let detection = ProjectDetection {
            app_type: AppType::Node,
            ..Default::default()
        };
        let app_type = AppType::from(&detection);
        assert_eq!(app_type, AppType::Node);
    }
}
