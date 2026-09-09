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
pub(crate) struct SkillDestinationStatus {
    pub(crate) name: String,
    pub(crate) current: usize,
    pub(crate) updates: usize,
    pub(crate) missing: usize,
    pub(crate) conflicts: usize,
}

impl SkillDestinationStatus {
    #[must_use]
    pub(crate) fn is_ready(&self) -> bool {
        self.updates == 0 && self.missing == 0 && self.conflicts == 0
    }

    #[must_use]
    pub(crate) fn has_conflicts(&self) -> bool {
        self.conflicts > 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SkillDestination {
    name: &'static str,
    skills_directory: PathBuf,
}

#[derive(Clone)]
struct SkillSnapshot {
    directory: PathBuf,
    existed: bool,
    skill_contents: Option<Vec<u8>>,
    marker_contents: Option<Vec<u8>>,
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
        self.validate_destinations()?;
        let snapshots = self.snapshots()?;
        match self.install_destinations() {
            Ok(installed) => Ok(installed),
            Err(error) => {
                let _rollback = restore_snapshots(&snapshots);
                Err(error)
            }
        }
    }

    pub(crate) fn inspect(
        &self,
    ) -> Result<Vec<SkillDestinationStatus>, CodexSkillInstallationError> {
        self.destinations.iter().map(inspect_destination).collect()
    }

    fn validate_destinations(&self) -> Result<(), CodexSkillInstallationError> {
        self.destinations
            .iter()
            .try_for_each(|destination| Self::validate_destination(destination).map(|_| ()))
    }

    fn snapshots(&self) -> Result<Vec<SkillSnapshot>, CodexSkillInstallationError> {
        self.destinations
            .iter()
            .flat_map(|destination| {
                BUNDLED_SKILLS
                    .iter()
                    .map(move |skill| snapshot(&destination.skills_directory, skill))
            })
            .collect()
    }

    fn install_destinations(&self) -> Result<Vec<String>, CodexSkillInstallationError> {
        self.destinations
            .iter()
            .map(|destination| {
                BUNDLED_SKILLS
                    .iter()
                    .try_for_each(|skill| install_skill(&destination.skills_directory, skill))?;
                Ok(destination.name.to_owned())
            })
            .collect()
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

fn inspect_destination(
    destination: &SkillDestination,
) -> Result<SkillDestinationStatus, CodexSkillInstallationError> {
    let states = BUNDLED_SKILLS
        .iter()
        .map(|skill| skill_state(&destination.skills_directory, skill))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SkillDestinationStatus {
        name: destination.name.to_owned(),
        current: states
            .iter()
            .filter(|state| **state == SkillState::Current)
            .count(),
        updates: states
            .iter()
            .filter(|state| **state == SkillState::Update)
            .count(),
        missing: states
            .iter()
            .filter(|state| **state == SkillState::Missing)
            .count(),
        conflicts: states
            .iter()
            .filter(|state| **state == SkillState::Conflict)
            .count(),
    })
}

#[derive(Eq, PartialEq)]
enum SkillState {
    Current,
    Update,
    Missing,
    Conflict,
}

fn skill_state(
    skills_directory: &Path,
    skill: &BundledSkill,
) -> Result<SkillState, CodexSkillInstallationError> {
    let directory = skills_directory.join(skill.name);
    if !directory.exists() {
        return Ok(SkillState::Missing);
    }
    if !directory.is_dir() || !marker_matches(&directory)? {
        return Ok(SkillState::Conflict);
    }
    let skill_path = directory.join("SKILL.md");
    match fs::read(&skill_path) {
        Ok(contents) if contents == skill.contents.as_bytes() => Ok(SkillState::Current),
        Ok(_) => Ok(SkillState::Update),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SkillState::Update),
        Err(source) => Err(storage(&skill_path, source)),
    }
}

fn snapshot(
    skills_directory: &Path,
    skill: &BundledSkill,
) -> Result<SkillSnapshot, CodexSkillInstallationError> {
    let directory = skills_directory.join(skill.name);
    Ok(SkillSnapshot {
        existed: directory.exists(),
        skill_contents: read_optional_file(&directory.join("SKILL.md"))?,
        marker_contents: read_optional_file(&directory.join(SKILL_MARKER_FILE))?,
        directory,
    })
}

fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>, CodexSkillInstallationError> {
    match fs::read(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(storage(path, source)),
    }
}

fn restore_snapshots(snapshots: &[SkillSnapshot]) -> Result<(), CodexSkillInstallationError> {
    snapshots.iter().rev().try_for_each(restore_snapshot)
}

fn restore_snapshot(snapshot: &SkillSnapshot) -> Result<(), CodexSkillInstallationError> {
    if !snapshot.existed {
        return remove_created_skill(&snapshot.directory);
    }
    restore_file(
        &snapshot.directory.join("SKILL.md"),
        snapshot.skill_contents.as_ref(),
    )?;
    restore_file(
        &snapshot.directory.join(SKILL_MARKER_FILE),
        snapshot.marker_contents.as_ref(),
    )
}

fn remove_created_skill(directory: &Path) -> Result<(), CodexSkillInstallationError> {
    if !directory.exists() {
        return Ok(());
    }
    fs::remove_dir_all(directory).map_err(|source| storage(directory, source))
}

fn restore_file(
    path: &Path,
    contents: Option<&Vec<u8>>,
) -> Result<(), CodexSkillInstallationError> {
    let Some(contents) = contents else {
        return remove_optional_file(path);
    };
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| storage(path, source))?;
    file.write_all(contents)
        .map_err(|source| storage(path, source))?;
    file.commit().map_err(|source| storage(path, source))
}

fn remove_optional_file(path: &Path) -> Result<(), CodexSkillInstallationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(storage(path, source)),
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

    use super::{AgentSkillInstaller, SKILL_MARKER_FILE};

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

    #[test]
    fn reports_missing_current_updates_and_conflicts() {
        let directory = TestDirectory::new();
        let skills = directory.path.join("skills");
        let installer = AgentSkillInstaller::at(skills.clone());
        let missing = installer.inspect().expect("missing skills status");
        assert_eq!(missing[0].missing, 3);

        installer.install().expect("skills install");
        let current = installer.inspect().expect("current skills status");
        assert!(current[0].is_ready());
        assert_eq!(current[0].current, 3);

        fs::write(skills.join("worklogger-jira/SKILL.md"), "outdated").expect("outdated skill");
        let update = installer.inspect().expect("update skills status");
        assert_eq!(update[0].updates, 1);

        fs::remove_file(skills.join("worklogger-daily").join(SKILL_MARKER_FILE))
            .expect("skill marker");
        let conflict = installer.inspect().expect("conflict skills status");
        assert!(conflict[0].has_conflicts());
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
