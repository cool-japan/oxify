use anyhow::{Context, Result};
use clap::Subcommand;
use oxify_connect_vision::{create_provider, VisionProvider, VisionProviderConfig};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Subcommand)]
pub enum VisionCommands {
    /// Process an image with OCR
    Process {
        /// Path to the image file
        image_path: String,
        /// OCR provider (mock, tesseract, surya, paddle)
        #[arg(short, long, default_value = "mock")]
        provider: String,
        /// Output format (text, markdown, json)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Language for OCR (en, ja, zh, etc.)
        #[arg(short, long)]
        language: Option<String>,
        /// Use GPU acceleration (if available)
        #[arg(long)]
        gpu: bool,
        /// Output file (optional)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// List available vision providers
    List,
    /// Show provider information
    Info {
        /// Provider name
        provider: String,
    },
    /// Benchmark vision providers on a test image
    Benchmark {
        /// Path to the test image
        image_path: String,
        /// Providers to benchmark (comma-separated, or "all")
        #[arg(short, long, default_value = "all")]
        providers: String,
        /// Number of iterations
        #[arg(short = 'n', long, default_value = "3")]
        iterations: u32,
    },
    /// Extract structured data from an image
    Extract {
        /// Path to the image file
        image_path: String,
        /// Data type to extract (text, table, form, receipt)
        #[arg(short, long, default_value = "text")]
        data_type: String,
        /// Provider to use
        #[arg(short, long, default_value = "surya")]
        provider: String,
    },
    /// Interactive OCR mode
    Interactive {
        /// OCR provider to use
        #[arg(short, long, default_value = "mock")]
        provider: String,
        /// Use GPU acceleration
        #[arg(long)]
        gpu: bool,
    },
    /// Process multiple images in batch
    Batch {
        /// Input file list (one image path per line) or directory
        input: String,
        /// OCR provider to use
        #[arg(short, long, default_value = "mock")]
        provider: String,
        /// Output directory for results
        #[arg(short, long)]
        output_dir: Option<String>,
        /// Maximum concurrent operations
        #[arg(short = 'c', long)]
        concurrency: Option<usize>,
        /// Output format (json, csv, text)
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Use GPU acceleration
        #[arg(long)]
        gpu: bool,
    },
    /// Watch a directory for new images and process them
    Watch {
        /// Directory to watch
        directory: String,
        /// OCR provider to use
        #[arg(short, long, default_value = "mock")]
        provider: String,
        /// Output directory for results
        #[arg(short, long)]
        output_dir: Option<String>,
        /// Use GPU acceleration
        #[arg(long)]
        gpu: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OcrOutput {
    pub provider: String,
    pub text: String,
    pub markdown: String,
    pub blocks: Vec<TextBlockOutput>,
    pub metadata: OcrMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TextBlockOutput {
    pub text: String,
    pub bbox: [f32; 4],
    pub confidence: f32,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OcrMetadata {
    pub processing_time_ms: u64,
    pub image_width: u32,
    pub image_height: u32,
    pub language: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub description: String,
    pub supported_formats: Vec<String>,
    pub supported_languages: Vec<String>,
    pub gpu_support: bool,
    pub requires_setup: bool,
    pub setup_instructions: Option<String>,
}

pub async fn handle_vision_command(command: VisionCommands) -> Result<()> {
    match command {
        VisionCommands::Process {
            image_path,
            provider,
            format,
            language,
            gpu,
            output,
        } => process_image(&image_path, &provider, &format, language, gpu, output).await,
        VisionCommands::List => list_providers().await,
        VisionCommands::Info { provider } => show_provider_info(&provider).await,
        VisionCommands::Benchmark {
            image_path,
            providers,
            iterations,
        } => benchmark_providers(&image_path, &providers, iterations).await,
        VisionCommands::Extract {
            image_path,
            data_type,
            provider,
        } => extract_data(&image_path, &data_type, &provider).await,
        VisionCommands::Interactive { provider, gpu } => interactive_mode(&provider, gpu).await,
        VisionCommands::Batch {
            input,
            provider,
            output_dir,
            concurrency,
            format,
            gpu,
        } => batch_process(&input, &provider, output_dir, concurrency, &format, gpu).await,
        VisionCommands::Watch {
            directory,
            provider,
            output_dir,
            gpu,
        } => watch_mode(&directory, &provider, output_dir, gpu).await,
    }
}

async fn process_image(
    image_path: &str,
    provider_name: &str,
    format: &str,
    language: Option<String>,
    gpu: bool,
    output: Option<String>,
) -> Result<()> {
    let path = Path::new(image_path);
    if !path.exists() {
        anyhow::bail!("Image file not found: {}", image_path);
    }

    println!("Processing image: {}", image_path);
    println!("Provider: {}", provider_name);
    if let Some(ref lang) = language {
        println!("Language: {}", lang);
    }

    // Read image file
    let image_data = std::fs::read(path)
        .with_context(|| format!("Failed to read image file: {}", image_path))?;

    // Create provider configuration
    let config = create_provider_config(provider_name, language.clone(), gpu)?;
    let provider = create_provider(&config)
        .with_context(|| format!("Failed to create provider: {}", provider_name))?;

    // Load model
    provider
        .load_model()
        .await
        .with_context(|| "Failed to load OCR model")?;

    // Process image
    let start = Instant::now();
    let ocr_result = provider
        .process_image(&image_data)
        .await
        .with_context(|| "Failed to process image")?;
    let processing_time = start.elapsed();

    // Convert to output format
    let (image_width, image_height) = ocr_result.metadata.image_size.unwrap_or((0, 0));
    let result = OcrOutput {
        provider: provider_name.to_string(),
        text: ocr_result.text.clone(),
        markdown: ocr_result.markdown.clone(),
        blocks: ocr_result
            .blocks
            .iter()
            .map(|b| TextBlockOutput {
                text: b.text.clone(),
                bbox: b.bbox,
                confidence: b.confidence,
                role: format!("{:?}", b.role),
            })
            .collect(),
        metadata: OcrMetadata {
            processing_time_ms: processing_time.as_millis() as u64,
            image_width,
            image_height,
            language: ocr_result.metadata.languages.first().cloned(),
        },
    };

    // Format output
    let output_text = match format.to_lowercase().as_str() {
        "text" => result.text.clone(),
        "markdown" => result.markdown.clone(),
        "json" => serde_json::to_string_pretty(&result)?,
        _ => anyhow::bail!(
            "Unknown format: {}. Available: text, markdown, json",
            format
        ),
    };

    // Write or print output
    if let Some(output_path) = output {
        std::fs::write(&output_path, &output_text)
            .with_context(|| format!("Failed to write output to: {}", output_path))?;
        println!("Output saved to: {}", output_path);
    } else {
        println!("\n--- Output ---\n{}", output_text);
    }

    println!("\nProcessing time: {:?}", processing_time);

    Ok(())
}

/// Create provider configuration from command-line arguments
fn create_provider_config(
    provider_name: &str,
    language: Option<String>,
    use_gpu: bool,
) -> Result<VisionProviderConfig> {
    let config = match provider_name.to_lowercase().as_str() {
        "mock" => VisionProviderConfig::mock(),
        "tesseract" => VisionProviderConfig::tesseract(language.as_deref()),
        "surya" => {
            // For now, use placeholder path - user should configure via config file
            VisionProviderConfig::surya("/path/to/surya/models", use_gpu)
        }
        "paddle" => {
            // For now, use placeholder path - user should configure via config file
            VisionProviderConfig::paddle("/path/to/paddle/models", use_gpu)
        }
        _ => anyhow::bail!(
            "Unknown provider: {}. Available: mock, tesseract, surya, paddle",
            provider_name
        ),
    };

    Ok(config)
}

async fn list_providers() -> Result<()> {
    println!("Available Vision Providers:");
    println!();
    println!("  mock       - Mock provider for testing (no dependencies)");
    println!("  tesseract  - Tesseract OCR (requires tesseract-ocr system package)");
    println!("  surya      - Surya OCR via ONNX Runtime (requires model files)");
    println!("  paddle     - PaddleOCR via ONNX Runtime (requires model files)");
    println!();
    println!("Use 'oxify vision info <provider>' for detailed information.");
    Ok(())
}

async fn show_provider_info(provider: &str) -> Result<()> {
    let info = match provider.to_lowercase().as_str() {
        "mock" => ProviderInfo {
            name: "Mock Vision Provider".to_string(),
            description: "A mock provider for testing and development. Returns placeholder OCR results.".to_string(),
            supported_formats: vec!["png".to_string(), "jpg".to_string(), "webp".to_string()],
            supported_languages: vec!["any".to_string()],
            gpu_support: false,
            requires_setup: false,
            setup_instructions: None,
        },
        "tesseract" => ProviderInfo {
            name: "Tesseract OCR".to_string(),
            description: "Open-source OCR engine maintained by Google. Supports 100+ languages.".to_string(),
            supported_formats: vec!["png".to_string(), "jpg".to_string(), "tiff".to_string(), "bmp".to_string()],
            supported_languages: vec!["eng".to_string(), "jpn".to_string(), "chi_sim".to_string(), "chi_tra".to_string(), "kor".to_string(), "deu".to_string(), "fra".to_string(), "spa".to_string()],
            gpu_support: false,
            requires_setup: true,
            setup_instructions: Some("Install tesseract-ocr:\n  Ubuntu/Debian: sudo apt install tesseract-ocr\n  macOS: brew install tesseract\n  Windows: https://github.com/UB-Mannheim/tesseract/wiki".to_string()),
        },
        "surya" => ProviderInfo {
            name: "Surya OCR".to_string(),
            description: "High-performance OCR using ONNX Runtime. Excellent for document analysis.".to_string(),
            supported_formats: vec!["png".to_string(), "jpg".to_string(), "webp".to_string()],
            supported_languages: vec!["en".to_string(), "ja".to_string(), "zh".to_string(), "ko".to_string(), "de".to_string(), "fr".to_string()],
            gpu_support: true,
            requires_setup: true,
            setup_instructions: Some("Download ONNX models from:\n  Detection: surya_det.onnx\n  Recognition: surya_rec.onnx\n\nSet model path in configuration.".to_string()),
        },
        "paddle" => ProviderInfo {
            name: "PaddleOCR".to_string(),
            description: "PaddlePaddle OCR via ONNX Runtime. Excellent for multilingual text.".to_string(),
            supported_formats: vec!["png".to_string(), "jpg".to_string(), "webp".to_string()],
            supported_languages: vec!["ch".to_string(), "en".to_string(), "japan".to_string(), "korean".to_string(), "german".to_string(), "french".to_string()],
            gpu_support: true,
            requires_setup: true,
            setup_instructions: Some("Download ONNX models from PaddleOCR releases:\n  Detection: ch_PP-OCRv4_det_infer.onnx\n  Recognition: ch_PP-OCRv4_rec_infer.onnx\n  Classification: ch_ppocr_mobile_v2.0_cls_infer.onnx".to_string()),
        },
        _ => anyhow::bail!("Unknown provider: {}", provider),
    };

    println!("Provider: {}", info.name);
    println!("Description: {}", info.description);
    println!();
    println!("Supported Formats: {}", info.supported_formats.join(", "));
    println!(
        "Supported Languages: {}",
        info.supported_languages.join(", ")
    );
    println!(
        "GPU Support: {}",
        if info.gpu_support { "Yes" } else { "No" }
    );
    println!(
        "Requires Setup: {}",
        if info.requires_setup { "Yes" } else { "No" }
    );

    if let Some(instructions) = info.setup_instructions {
        println!();
        println!("Setup Instructions:");
        for line in instructions.lines() {
            println!("  {}", line);
        }
    }

    Ok(())
}

async fn benchmark_providers(image_path: &str, providers_str: &str, iterations: u32) -> Result<()> {
    let path = Path::new(image_path);
    if !path.exists() {
        anyhow::bail!("Image file not found: {}", image_path);
    }

    // Read image data
    let image_data = std::fs::read(path)
        .with_context(|| format!("Failed to read image file: {}", image_path))?;

    let provider_list: Vec<&str> = if providers_str == "all" {
        vec!["mock"] // Only benchmark mock provider by default
    } else {
        providers_str.split(',').map(|s| s.trim()).collect()
    };

    println!("Benchmarking Vision Providers");
    println!("Image: {}", image_path);
    println!("Image size: {} bytes", image_data.len());
    println!("Iterations: {}", iterations);
    println!();

    println!(
        "{:<12} {:>8} {:>8} {:>8} {:>8}",
        "Provider", "Iters", "Min (ms)", "Avg (ms)", "Max (ms)"
    );
    println!("{}", "─".repeat(52));

    for provider_name in provider_list {
        let config = match create_provider_config(provider_name, None, false) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to create config for {}: {}",
                    provider_name, e
                );
                continue;
            }
        };

        let provider = match create_provider(&config) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to create provider {}: {}",
                    provider_name, e
                );
                continue;
            }
        };

        if provider.load_model().await.is_err() {
            eprintln!("Warning: Failed to load model for {}", provider_name);
            continue;
        }

        // Run benchmark iterations
        let mut times = Vec::new();
        for _ in 0..iterations {
            let start = Instant::now();
            match provider.process_image(&image_data).await {
                Ok(_) => {
                    times.push(start.elapsed().as_millis() as u64);
                }
                Err(e) => {
                    eprintln!("Warning: Benchmark iteration failed: {}", e);
                    break;
                }
            }
        }

        if times.is_empty() {
            eprintln!("Warning: No successful iterations for {}", provider_name);
            continue;
        }

        let min = *times.iter().min().expect("invariant: times non-empty after guard");
        let max = *times.iter().max().expect("invariant: times non-empty after guard");
        let avg: u64 = times.iter().sum::<u64>() / times.len() as u64;

        println!(
            "{:<12} {:>8} {:>8} {:>8} {:>8}",
            provider_name, iterations, min, avg, max
        );
    }

    println!();

    Ok(())
}

async fn extract_data(image_path: &str, data_type: &str, provider: &str) -> Result<()> {
    let path = Path::new(image_path);
    if !path.exists() {
        anyhow::bail!("Image file not found: {}", image_path);
    }

    println!("Extracting {} data from: {}", data_type, image_path);
    println!("Provider: {}", provider);
    println!();

    match data_type.to_lowercase().as_str() {
        "text" => {
            println!("Extracted Text:");
            println!("  [Text extraction requires provider setup]");
        }
        "table" => {
            println!("Extracted Table:");
            println!("  | Column 1 | Column 2 | Column 3 |");
            println!("  |----------|----------|----------|");
            println!("  | [setup]  | [needed] | [...]    |");
        }
        "form" => {
            println!("Extracted Form Fields:");
            println!("  Field 1: [value]");
            println!("  Field 2: [value]");
            println!("  [Form extraction requires provider setup]");
        }
        "receipt" => {
            println!("Extracted Receipt Data:");
            println!("  Store: [unknown]");
            println!("  Date: [unknown]");
            println!("  Total: [unknown]");
            println!("  [Receipt extraction requires provider setup]");
        }
        _ => {
            anyhow::bail!(
                "Unknown data type: {}. Available: text, table, form, receipt",
                data_type
            );
        }
    }

    Ok(())
}

/// Interactive OCR mode with REPL-style interface
async fn interactive_mode(provider_name: &str, gpu: bool) -> Result<()> {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║  OxiFY Vision - Interactive OCR Mode                      ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("Provider: {}", provider_name);
    println!("GPU: {}", if gpu { "enabled" } else { "disabled" });
    println!();
    println!("Commands:");
    println!("  process <image_path>  - Process an image");
    println!("  info                  - Show provider information");
    println!("  help                  - Show this help message");
    println!("  quit / exit           - Exit interactive mode");
    println!();

    // Create and load provider
    let config = create_provider_config(provider_name, None, gpu)?;
    let provider = create_provider(&config)
        .with_context(|| format!("Failed to create provider: {}", provider_name))?;

    provider
        .load_model()
        .await
        .with_context(|| "Failed to load OCR model")?;

    println!("✓ Provider loaded successfully");
    println!();

    // Interactive loop
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts.first().copied().unwrap_or("");

        match command {
            "quit" | "exit" => {
                println!("Goodbye!");
                break;
            }
            "help" => {
                println!("Commands:");
                println!("  process <image_path>  - Process an image");
                println!("  info                  - Show provider information");
                println!("  help                  - Show this help message");
                println!("  quit / exit           - Exit interactive mode");
            }
            "info" => {
                println!("Provider: {}", provider.provider_name());
                let caps = provider.capabilities();
                println!("Layout analysis: {}", caps.layout_analysis);
                println!("Table detection: {}", caps.table_detection);
                println!("Handwriting: {}", caps.handwriting);
                println!("Multi-language: {}", caps.multi_language);
                println!("GPU acceleration: {}", caps.gpu_acceleration);
                println!("Languages: {}", caps.languages.join(", "));
            }
            "process" => {
                if parts.len() < 2 {
                    eprintln!("Error: Missing image path. Usage: process <image_path>");
                    continue;
                }

                let image_path = parts[1];
                match process_image_interactive(&*provider, image_path).await {
                    Ok(_) => {}
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            _ => {
                eprintln!(
                    "Unknown command: {}. Type 'help' for available commands.",
                    command
                );
            }
        }

        println!();
    }

    Ok(())
}

/// Process image in interactive mode
async fn process_image_interactive(provider: &dyn VisionProvider, image_path: &str) -> Result<()> {
    let path = Path::new(image_path);
    if !path.exists() {
        anyhow::bail!("Image file not found: {}", image_path);
    }

    let image_data =
        std::fs::read(path).with_context(|| format!("Failed to read image: {}", image_path))?;

    println!("Processing: {} ({} bytes)", image_path, image_data.len());

    let start = Instant::now();
    let result = provider
        .process_image(&image_data)
        .await
        .with_context(|| "Failed to process image")?;
    let elapsed = start.elapsed();

    println!("✓ Processing complete in {:?}", elapsed);
    println!();
    println!("Extracted Text:");
    println!("{}", result.text);
    println!();
    println!("Blocks: {}", result.blocks.len());
    println!("Characters: {}", result.text.chars().count());

    Ok(())
}

/// Batch process images from a file list or directory
async fn batch_process(
    input: &str,
    provider_name: &str,
    output_dir: Option<String>,
    _concurrency: Option<usize>,
    format: &str,
    gpu: bool,
) -> Result<()> {
    println!("Batch Processing");
    println!("Input: {}", input);
    println!("Provider: {}", provider_name);
    println!();

    // Determine if input is a file list or directory
    let input_path = Path::new(input);
    let image_paths: Vec<PathBuf> = if input_path.is_dir() {
        // Scan directory for image files
        std::fs::read_dir(input_path)?
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    let path = e.path();
                    if path.is_file() {
                        let ext = path.extension()?.to_str()?;
                        if matches!(
                            ext.to_lowercase().as_str(),
                            "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tiff"
                        ) {
                            Some(path)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            })
            .collect()
    } else if input_path.is_file() {
        // Read file list (one path per line)
        let content = std::fs::read_to_string(input_path)?;
        content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| PathBuf::from(line.trim()))
            .collect()
    } else {
        anyhow::bail!("Input must be a directory or a file containing image paths");
    };

    if image_paths.is_empty() {
        anyhow::bail!("No images found in input");
    }

    println!("Found {} images to process", image_paths.len());

    // Create provider
    let config = create_provider_config(provider_name, None, gpu)?;
    let provider = create_provider(&config)?;
    provider.load_model().await?;

    // Create output directory if specified
    if let Some(ref out_dir) = output_dir {
        std::fs::create_dir_all(out_dir)?;
        println!("Output directory: {}", out_dir);
    }

    println!();
    println!("Processing {} images...", image_paths.len());
    let start = Instant::now();

    // Process images sequentially (simple implementation)
    let mut successful = 0;
    let mut failed = 0;
    let mut results = Vec::new();

    for (idx, image_path) in image_paths.iter().enumerate() {
        match std::fs::read(image_path) {
            Ok(image_data) => match provider.process_image(&image_data).await {
                Ok(ocr_result) => {
                    results.push(Some(ocr_result));
                    successful += 1;
                }
                Err(e) => {
                    eprintln!("Error processing {}: {}", image_path.display(), e);
                    results.push(None);
                    failed += 1;
                }
            },
            Err(e) => {
                eprintln!("Failed to read {}: {}", image_path.display(), e);
                results.push(None);
                failed += 1;
            }
        }

        // Progress indicator
        if (idx + 1) % 10 == 0 {
            println!("Processed {}/{}", idx + 1, image_paths.len());
        }
    }

    let elapsed = start.elapsed();

    println!();
    println!("Batch processing complete in {:?}", elapsed);
    println!("Successful: {}", successful);
    println!("Failed: {}", failed);

    // Calculate throughput
    let elapsed_secs = elapsed.as_secs_f64();
    let throughput = if elapsed_secs > 0.0 {
        successful as f64 / elapsed_secs
    } else {
        0.0
    };
    println!("Throughput: {:.2} images/sec", throughput);

    // Save results
    if let Some(out_dir) = output_dir {
        for (idx, ocr_result_opt) in results.iter().enumerate() {
            if let Some(ref ocr_result) = ocr_result_opt {
                let output_path = Path::new(&out_dir).join(format!("result_{:04}.{}", idx, format));
                let content = match format {
                    "text" => ocr_result.text.clone(),
                    "markdown" => ocr_result.markdown.clone(),
                    "json" => serde_json::to_string_pretty(ocr_result)?,
                    _ => ocr_result.text.clone(),
                };

                std::fs::write(&output_path, content)?;
            }
        }
        println!("Results saved to: {}", out_dir);
    }

    Ok(())
}

/// Watch directory for new images and process them automatically
async fn watch_mode(
    directory: &str,
    provider_name: &str,
    output_dir: Option<String>,
    gpu: bool,
) -> Result<()> {
    println!("Watch Mode");
    println!("Directory: {}", directory);
    println!("Provider: {}", provider_name);
    println!();

    let dir_path = Path::new(directory);
    if !dir_path.exists() {
        anyhow::bail!("Directory not found: {}", directory);
    }
    if !dir_path.is_dir() {
        anyhow::bail!("Path is not a directory: {}", directory);
    }

    // Create provider
    let config = create_provider_config(provider_name, None, gpu)?;
    let provider = create_provider(&config)?;
    provider.load_model().await?;

    println!("✓ Provider loaded successfully");

    // Create output directory if specified
    if let Some(ref out_dir) = output_dir {
        std::fs::create_dir_all(out_dir)?;
        println!("Output directory: {}", out_dir);
    }

    println!();
    println!("Watching for new images... (Press Ctrl+C to stop)");
    println!();

    // Track processed files
    let mut processed_files = std::collections::HashSet::new();

    // Simple polling-based watch (for basic implementation)
    loop {
        // Scan directory
        let entries: Vec<_> = std::fs::read_dir(dir_path)?
            .filter_map(|entry| entry.ok())
            .collect();

        for entry in entries {
            let path = entry.path();

            // Check if it's an image file we haven't processed
            if path.is_file() && !processed_files.contains(&path) {
                if let Some(ext) = path.extension() {
                    if matches!(
                        ext.to_str().unwrap_or("").to_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tiff"
                    ) {
                        println!("New image detected: {}", path.display());

                        // Process the image
                        match process_watched_image(&*provider, &path, &output_dir).await {
                            Ok(_) => {
                                println!("✓ Processed successfully");
                                processed_files.insert(path);
                            }
                            Err(e) => {
                                eprintln!("✗ Processing failed: {}", e);
                            }
                        }
                        println!();
                    }
                }
            }
        }

        // Sleep before next scan
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

/// Process a watched image file
async fn process_watched_image(
    provider: &dyn VisionProvider,
    image_path: &Path,
    output_dir: &Option<String>,
) -> Result<()> {
    let image_data = std::fs::read(image_path)?;

    let start = Instant::now();
    let result = provider.process_image(&image_data).await?;
    let elapsed = start.elapsed();

    println!("Processing time: {:?}", elapsed);
    println!("Extracted {} characters", result.text.chars().count());

    // Save results if output directory specified
    if let Some(out_dir) = output_dir {
        let file_stem = image_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");

        let json_path = Path::new(out_dir).join(format!("{}.json", file_stem));
        let txt_path = Path::new(out_dir).join(format!("{}.txt", file_stem));

        // Save JSON
        let json = serde_json::to_string_pretty(&result)?;
        std::fs::write(&json_path, json)?;

        // Save text
        std::fs::write(&txt_path, &result.text)?;

        println!("Saved to: {}", out_dir);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocr_output_serialization() {
        let output = OcrOutput {
            provider: "mock".to_string(),
            text: "Hello World".to_string(),
            markdown: "# Hello World".to_string(),
            blocks: vec![],
            metadata: OcrMetadata {
                processing_time_ms: 10,
                image_width: 800,
                image_height: 600,
                language: Some("en".to_string()),
            },
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("Hello World"));
    }

    #[test]
    fn test_provider_info_serialization() {
        let info = ProviderInfo {
            name: "Test".to_string(),
            description: "Test provider".to_string(),
            supported_formats: vec!["png".to_string()],
            supported_languages: vec!["en".to_string()],
            gpu_support: true,
            requires_setup: false,
            setup_instructions: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Test"));
    }
}
