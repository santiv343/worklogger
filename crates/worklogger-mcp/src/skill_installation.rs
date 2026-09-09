use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
use thiserror::Error;

const HOME_ENVIRONMENT_VARIABLE: &str = "HOME";
const USER_PROFILE_ENVIRONMENT_VARIABLE: &str = "USERPROFILE";
const CODEX_HOME_ENVIRONMENT_VARIABLE: &str = "CODEX_HOME";
const CODEX_DIRECTORY: &str = ".codex";
const CLAUDE_DIRECTORY: &str = ".claude";
const AGENTS_DIRECTORY: &str = ".agents";
const WINDSURF_DIRECTORY: &str = ".codeium/windsurf";
const SKILLS_DIRECTORY: &str = "skills";
const SKILL_MARKER_FILE: &str = ".worklogger-skill";
const SKILL_MARKER: &str = "worklogger\n";

const BUNDLED_SKILLS: &[BundledSkill] = &[
    BundledSkill {
        name: "worklogger-jira",
        contents: include_str!("../resources/skills/worklogger-jira/SKILL.md"),
    },
    BundledSkill {
        name: "worklogger-daily",
        contents: include_str!("../resources/skills/worklogger-daily/SKILL.md"),
    },
    BundledSkill {
        name: "worklogger-delivery",
        contents: include_str!("../resources/skills/worklogger-delivery/SKILL.md"),
    },
];

struct BundledSkill {
    name: &'static str,
    contents: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentSkillInstaller {
    destinations: Vec<SkillDestination>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SkillDestination {
    name: &'static str,
    skills_directory: PathBuf,
}

#[derive(Debug, Error)]
pub(crate) enum CodexSkillInstallationError {
    #[error("no se pudo determinar el directorio de skills del asistente")]
    MissingSkillsDirectory,
    #[error("el directorio de skills debe ser una ruta absoluta: {0}")]
    InvalidSkillsDirectory(PathBuf),
    #[error("la skill existente no pertenece a Worklogger: {0}")]
    UnownedSkill(PathBuf),
    #[error("no se pudo instalar la skill en {path}: {source}")]
    Storage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl AgentSkillInstaller {
    pub(crate) fn for_current_user() -> Result<Self, CodexSkillInstallationError> {
        current_user_skill_destinations()
            .map(Self::with_destinations)
            .ok_or(CodexSkillInstallationError::MissingSkillsDirectory)
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn at(skills_directory: PathBuf) -> Self {
        Self::with_destinations(vec![SkillDestination {
            name: "Prueba",
            skills_directory,
        }])
    }

    #[must_use]
    fn with_destinations(destinations: Vec<SkillDestination>) -> Self {
        Self { destinations }
    }

    pub(crate) fn install(&self) -> Result<Vec<String>, CodexSkillInstallationError> {
        self.destinations
            .iter()
            .map(Self::install_destination)
            .collect()
    }

    fn install_destination(
        destination: &SkillDestination,
    ) -> Result<String, CodexSkillInstallationError> {
        Self::validate_destination(destination)?;
        BUNDLED_SKILLS
            .iter()
            .try_for_each(|skill| install_skill(&destination.skills_directory, skill))?;
        Ok(destination.name.to_owned())
    }

    fn validate_destination(
        destination: &SkillDestination,
    ) -> Result<PathBuf, CodexSkillInstallationError> {
        validate_skills_directory(&destination.skills_directory)?;
        BUNDLED_SKILLS
            .iter()
            .try_for_each(|skill| validate_skill_target(&destination.skills_directory, skill))?;
        Ok(destination.skills_directory.clone())
    }
}

fn current_user_skill_destinations() -> Option<Vec<SkillDestination>> {
    let home = current_user_home()?;
    let codex = environment_path(CODEX_HOME_ENVIRONMENT_VARIABLE)
        .unwrap_or_else(|| home.join(CODEX_DIRECTORY));
    Some(vec![
        destination("Agent Skills", &home.join(AGENTS_DIRECTORY)),
        destination("Codex", &codex),
        destination("Claude Code", &home.join(CLAUDE_DIRECTORY)),
        destination("Windsurf", &home.join(WINDSURF_DIRECTORY)),
    ])
}

fn current_user_home() -> Option<PathBuf> {
    if cfg!(windows) {
        environment_path(USER_PROFILE_ENVIRONMENT_VARIABLE)
    } else {
        environment_path(HOME_ENVIRONMENT_VARIABLE)
    }
}

fn destination(name: &'static str, root: &Path) -> SkillDestination {
    SkillDestination {
        name,
        skills_directory: root.join(SKILLS_DIRECTORY),
    }
}

fn environment_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn validate_skills_directory(path: &Path) -> Result<(), CodexSkillInstallationError> {
    if path.is_absolute() {
        return Ok(());
    }
    Err(CodexSkillInstallationError::InvalidSkillsDirectory(
        path.to_path_buf(),
    ))
}

fn validate_skill_target(
    skills_directory: &Path,
    skill: &BundledSkill,
) -> Result<(), CodexSkillInstallationError> {
    let path = skills_directory.join(skill.name);
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() && marker_matches(&path)? {
        return Ok(());
    }
    Err(CodexSkillInstallationError::UnownedSkill(path))
}

fn install_skill(
    skills_directory: &Path,
    skill: &BundledSkill,
) -> Result<(), CodexSkillInstallationError> {
    let directory = skills_directory.join(skill.name);
    fs::create_dir_all(&directory).map_err(|source| storage(&directory, source))?;
    write_file(&directory.join("SKILL.md"), skill.contents)?;
    write_file(&directory.join(SKILL_MARKER_FILE), SKILL_MARKER)
}

fn marker_matches(directory: &Path) -> Result<bool, CodexSkillInstallationError> {
    let marker = directory.join(SKILL_MARKER_FILE);
    fs::read_to_string(&marker)
        .map(|contents| contents == SKILL_MARKER)
        .or_else(missing_marker_is_false)
        .map_err(|source| storage(&marker, source))
}

fn missing_marker_is_false(error: std::io::Error) -> Result<bool, std::io::Error> {
    if error.kind() == std::io::ErrorKind::NotFound {
        return Ok(false);
    }
    Err(error)
}

fn write_file(path: &Path, contents: &str) -> Result<(), CodexSkillInstallationError> {
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| storage(path, source))?;
    file.write_all(contents.as_bytes())
        .map_err(|source| storage(path, source))?;
    file.commit().map_err(|source| storage(path, source))
}

fn storage(path: &Path, source: std::io::Error) -> CodexSkillInstallationError {
    CodexSkillInstallationError::Storage {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::AgentSkillInstaller;

    #[test]
    fn installs_only_worklogger_owned_skills() {
        let directory = TestDirectory::new();
        let skills = directory.path.join("skills");
        let unrelated = skills.join("user-skill");
        fs::create_dir_all(&unrelated).expect("unrelated directory");
        fs::write(unrelated.join("SKILL.md"), "user content").expect("unrelated skill");

        let installed = AgentSkillInstaller::at(skills.clone())
            .install()
            .expect("skills install");

        assert_eq!(installed, ["Prueba"]);
        assert_eq!(
            fs::read_to_string(unrelated.join("SKILL.md")).expect("unrelated skill remains"),
            "user content"
        );
        assert!(skills.join("worklogger-jira/SKILL.md").is_file());
    }

    #[test]
    fn refuses_to_replace_an_unowned_worklogger_skill() {
        let directory = TestDirectory::new();
        let skills = directory.path.join("skills");
        let target = skills.join("worklogger-jira");
        fs::create_dir_all(&target).expect("target directory");
        fs::write(target.join("SKILL.md"), "other content").expect("other skill");

        assert!(
            AgentSkillInstaller::at(skills)
                .install()
                .expect_err("unowned skill must fail")
                .to_string()
                .contains("no pertenece a Worklogger")
        );
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("worklogger-skills-{sequence}"));
            fs::create_dir_all(&path).expect("test directory");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.path).expect("test directory cleanup");
        }
    }
}
