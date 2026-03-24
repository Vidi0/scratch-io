use crate::eprintln_exit;

use clap::Subcommand;
use std::path::{Path, PathBuf};

// These are calls to wharf commands (patch, verify)
#[derive(Subcommand)]
pub enum WharfCommand {
  /// Print info about a given wharf binary file
  ///
  /// The statistics printed by default are the kind of file (signature or
  /// patch), the compression used on it and the number of files,
  /// directories and symlinks present
  ///
  /// Calling it with the dump option could be useful to return all the
  /// data inside the binary file into a human-readable output.
  Info {
    /// The path where the wharf binary file to identify is located
    wharf_file: PathBuf,

    /// Dump all the data in the binary file, not only the header
    /// and container information
    ///
    /// Warning! If the binary file is large, dumping the data could lead
    /// to an immense output, potentially 100× the size of the original file,
    /// depending on the contents.
    #[arg(long, env = "SCRATCH_DUMP")]
    dump: bool,
  },

  /// Verify that the provided build folder is intact
  ///
  /// Returns an struct containing the indexes in the signature
  /// of the broken or missing files
  Verify {
    /// The path where the wharf signature file is placed
    signature_file: PathBuf,
    /// The path where the build folder is located
    #[arg(long, env = "SCRATCH_BUILD_FOLDER")]
    build_folder: PathBuf,
  },
  /// Verify and repair the provided build folder
  ///
  /// Runs the verification command, and then replaces all the
  /// broken and missing files with the ones from the provided
  /// ZIP archive.
  ///
  /// This command is usless by itself, because if the ZIP
  /// archive has been fully downloaded, it is better to extract
  /// it directly and replace the old build folder.
  Repair {
    /// The path where the wharf signature file is placed
    signature_file: PathBuf,
    /// The path where the build folder is located
    #[arg(long, env = "SCRATCH_BUILD_FOLDER")]
    build_folder: PathBuf,
    /// The path to the build's ZIP archive
    #[arg(long, env = "SCRATCH_ZIP_ARCHIVE")]
    zip_archive: PathBuf,
  },
  /// Apply a wharf patch from an old build folder into a new one
  ///
  /// If a signature file is provided, verify the patch integrity
  /// on the fly while it is beign applied and cancel the patching
  /// if the new build folder is going to be corrupted.
  Patch {
    /// The path where the wharf patch file is placed
    patch_file: PathBuf,
    /// The path where the wharf signature file is placed
    ///
    /// If it isn't provided, don't check the patch integrity.
    #[arg(long, env = "SCRATCH_SIGNATURE_FILE")]
    signature_file: Option<PathBuf>,
    /// The path where the old build folder is located
    ///
    /// All files in this folder will remain intact after
    /// applying the patch.
    #[arg(long, env = "SCRATCH_OLD_BUILD_FOLDER")]
    old_build_folder: PathBuf,
    /// The path where the half-reconstructed files will be placed
    ///
    /// Data in this folder may be overwritten
    #[arg(long, env = "SCRATCH_OLD_BUILD_FOLDER")]
    staging_folder: PathBuf,
    /// The path where the new build folder will be placed
    ///
    /// Existing files will be overwritten by the new ones,
    /// but those but not present on the patch file will
    /// remain intact.
    #[arg(long, env = "SCRATCH_NEW_BUILD_FOLDER")]
    new_build_folder: PathBuf,
  },
}

fn info(wharf_file: &Path, dump: bool) {
  // Open the wharf file
  let mut file = std::io::BufReader::new(
    std::fs::File::open(wharf_file).unwrap_or_else(|e| eprintln_exit!("{e}")),
  );

  let mut binary = wharf::info::identify(&mut file).unwrap_or_else(|e| eprintln_exit!("{e}"));

  if dump {
    binary
      .dump_stdout()
      .unwrap_or_else(|e| eprintln_exit!("{e}"));
  } else {
    binary.print_summary();
  }
}

fn verify(signature_file: &Path, build_folder: &Path) {
  // Open the signature file
  let mut file = std::io::BufReader::new(
    std::fs::File::open(signature_file).unwrap_or_else(|e| eprintln_exit!("{e}")),
  );

  // Read the signature
  let mut signature = wharf::Signature::read(&mut file).unwrap_or_else(|e| eprintln_exit!("{e}"));

  // Set up the progress bar
  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
          indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) ({eta})").unwrap()
            .progress_chars("#>-")
        );
  progress_bar.set_length(signature.container_new.size as u64);
  progress_bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());

  // Do the files verification
  let broken = signature
    .verify_files(build_folder, |b| progress_bar.inc(b))
    .unwrap_or_else(|e| eprintln_exit!("{e}"));

  progress_bar.finish();

  println!("{broken:#?}");
}

fn repair(signature_file: &Path, build_folder: &Path, zip_archive: &Path) {
  use rc_zip_sync::ReadZip;

  // -- VERIFY FILES --

  // Open the signature file
  let mut file = std::io::BufReader::new(
    std::fs::File::open(signature_file).unwrap_or_else(|e| eprintln_exit!("{e}")),
  );

  // Read the signature
  let mut signature = wharf::Signature::read(&mut file).unwrap_or_else(|e| eprintln_exit!("{e}"));

  // Set up the progress bar
  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
          indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) ({eta})").unwrap()
            .progress_chars("#>-")
        );
  progress_bar.set_length(signature.container_new.size as u64);
  progress_bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());

  // Do the files verification
  let broken = signature
    .verify_files(build_folder, |b| progress_bar.inc(b))
    .unwrap_or_else(|e| eprintln_exit!("{e}"));

  progress_bar.finish();

  // -- REPAIR DAMAGED FILES --

  // Open the ZIP file
  let zip_file = std::fs::File::open(zip_archive).unwrap_or_else(|e| eprintln_exit!("{e}"));
  let zip = zip_file
    .read_zip()
    .unwrap_or_else(|e| eprintln_exit!("{e}"));

  // Set up the progress bar
  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
          indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) ({eta})").unwrap()
            .progress_chars("#>-")
        );
  progress_bar.set_length(broken.bytes_to_fix(&signature.container_new));
  progress_bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());

  // Repair the damaged files
  signature
    .repair(&broken, build_folder, &zip, |b| progress_bar.inc(b))
    .unwrap_or_else(|e| eprintln_exit!("{e}"));

  progress_bar.finish();
}

fn patch(
  patch_file: &Path,
  signature_file: Option<&Path>,
  old_build_folder: &Path,
  staging_folder: &Path,
  new_build_folder: &Path,
) {
  // Open the patch file
  let mut file = std::io::BufReader::new(
    std::fs::File::open(patch_file).unwrap_or_else(|e| eprintln_exit!("{e}")),
  );

  // Read the patch
  let mut patch = wharf::Patch::read(&mut file).unwrap_or_else(|e| eprintln_exit!("{e}"));

  // Open the signature file
  let mut signature_file = signature_file.map(|signature_file| {
    std::io::BufReader::new(
      std::fs::File::open(signature_file).unwrap_or_else(|e| eprintln_exit!("{e}")),
    )
  });

  // Read the signature
  let mut hash_iter = signature_file.as_mut().map(|signature_file| {
    wharf::Signature::read(signature_file)
      .unwrap_or_else(|e| eprintln_exit!("{e}"))
      .block_hash_iter
  });

  // Set up the progress bar
  let progress_bar = indicatif::ProgressBar::hidden();
  progress_bar.set_style(
          indicatif::ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) ({eta})").unwrap()
            .progress_chars("#>-")
        );
  progress_bar.set_length(patch.container_new.size as u64);
  progress_bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());

  // Apply the patch
  patch
    .apply(
      old_build_folder,
      staging_folder,
      new_build_folder,
      hash_iter.as_mut(),
      |b| progress_bar.inc(b),
    )
    .unwrap_or_else(|e| eprintln_exit!("{e}"));

  progress_bar.finish();
}

impl WharfCommand {
  pub fn handle_command(self) {
    match self {
      Self::Info { wharf_file, dump } => info(&wharf_file, dump),
      Self::Verify {
        signature_file,
        build_folder,
      } => verify(&signature_file, &build_folder),
      Self::Repair {
        signature_file,
        build_folder,
        zip_archive,
      } => repair(&signature_file, &build_folder, &zip_archive),
      Self::Patch {
        patch_file,
        signature_file,
        old_build_folder,
        staging_folder,
        new_build_folder,
      } => patch(
        &patch_file,
        signature_file.as_deref(),
        &old_build_folder,
        &staging_folder,
        &new_build_folder,
      ),
    }
  }
}
