//! Vision/OCR benchmark suite.
//!
//! Provides automated performance testing, provider comparisons,
//! memory profiling, and latency measurements.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxify_connect_vision::{
    create_provider, providers::MockVisionProvider, ImagePreprocessor, PreprocessConfig,
    VisionProvider, VisionProviderConfig,
};
use std::hint::black_box;

/// Sample test image (small PNG-like data)
fn sample_image() -> Vec<u8> {
    // Create a simple test image (just bytes for benchmarking)
    vec![0u8; 1024 * 100] // 100KB test image
}

/// Benchmark mock provider OCR processing
fn bench_mock_provider(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = VisionProviderConfig::mock();
    let provider = create_provider(&config).unwrap();
    rt.block_on(provider.load_model()).unwrap();

    let image = sample_image();

    c.bench_function("mock_provider_ocr", |b| {
        b.to_async(&rt).iter(|| async {
            let result = provider.process_image(black_box(&image)).await;
            black_box(result)
        })
    });
}

/// Benchmark image preprocessing operations
fn bench_preprocessing(c: &mut Criterion) {
    let mut group = c.benchmark_group("preprocessing");

    let image_data = sample_image();
    let image = image::load_from_memory(&image_data)
        .unwrap_or_else(|_| image::DynamicImage::ImageRgb8(image::RgbImage::new(800, 600)));

    // Benchmark resize
    group.bench_function("resize", |b| {
        b.iter(|| {
            let resized = oxify_connect_vision::preprocessing::resize_if_needed(
                black_box(image.clone()),
                2048,
            );
            black_box(resized)
        })
    });

    // Benchmark denoise
    group.bench_function("denoise", |b| {
        b.iter(|| {
            let denoised =
                oxify_connect_vision::preprocessing::denoise_image(black_box(image.clone()));
            black_box(denoised)
        })
    });

    // Benchmark contrast enhancement
    group.bench_function("enhance_contrast", |b| {
        b.iter(|| {
            let enhanced =
                oxify_connect_vision::preprocessing::enhance_contrast(black_box(image.clone()));
            black_box(enhanced)
        })
    });

    // Benchmark full preprocessing pipeline
    let preprocessor = ImagePreprocessor::new(PreprocessConfig::high_quality());
    group.bench_function("full_pipeline", |b| {
        b.iter(|| {
            let processed = preprocessor.preprocess(black_box(image.clone()));
            black_box(processed)
        })
    });

    group.finish();
}

/// Benchmark batch processing with different concurrency levels
fn bench_batch_processing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("batch_processing");

    let provider =
        std::sync::Arc::new(MockVisionProvider::new()) as std::sync::Arc<dyn VisionProvider>;
    rt.block_on(provider.load_model()).unwrap();

    // Generate test images
    let images: Vec<Vec<u8>> = (0..10).map(|_| sample_image()).collect();

    for concurrency in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("concurrent_ocr", concurrency),
            &concurrency,
            |b, &concurrency| {
                let provider = provider.clone();
                b.to_async(&rt).iter(|| {
                    let provider = provider.clone();
                    let images = images.clone();
                    async move {
                        let batch_config = oxify_connect_vision::BatchConfig::default()
                            .with_max_concurrency(concurrency);
                        let processor = oxify_connect_vision::BatchProcessor::new(batch_config);

                        let result = processor.process_batch(provider, black_box(images)).await;
                        black_box(result)
                    }
                })
            },
        );
    }

    group.finish();
}

/// Benchmark cache operations
fn bench_cache(c: &mut Criterion) {
    use oxify_connect_vision::CacheKey;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = VisionProviderConfig::mock();
    let provider = create_provider(&config).unwrap();
    rt.block_on(provider.load_model()).unwrap();

    let cache = oxify_connect_vision::VisionCache::with_max_size(1000);

    let image = sample_image();
    let cache_key = CacheKey::new(&image, "mock", "text", None);
    let nonexistent_key = CacheKey::new(b"nonexistent", "mock", "text", None);

    // Pre-populate cache
    let result = rt.block_on(provider.process_image(&image)).unwrap();
    cache.set(cache_key.clone(), result);

    c.bench_function("cache_hit", |b| {
        b.iter(|| {
            let cached = cache.get(black_box(&cache_key));
            black_box(cached)
        })
    });

    c.bench_function("cache_miss", |b| {
        b.iter(|| {
            let cached = cache.get(black_box(&nonexistent_key));
            black_box(cached)
        })
    });
}

/// Benchmark model downloader operations
fn bench_downloader(c: &mut Criterion) {
    use oxify_connect_vision::{DownloaderConfig, ModelDownloader};

    let config = DownloaderConfig::new()
        .with_verify_checksums(true)
        .with_report_progress(false);

    c.bench_function("downloader_creation", |b| {
        b.iter(|| {
            let downloader = ModelDownloader::new(black_box(config.clone()));
            black_box(downloader)
        })
    });
}

/// Benchmark GPU detection
fn bench_gpu_detection(c: &mut Criterion) {
    use oxify_connect_vision::GpuInfo;

    c.bench_function("gpu_info_detect", |b| {
        b.iter(|| {
            let info = GpuInfo::detect();
            black_box(info)
        })
    });

    c.bench_function("gpu_info_cached", |b| {
        b.iter(|| {
            let info = GpuInfo::cached();
            black_box(info)
        })
    });
}

/// Benchmark diagnostics generation
fn bench_diagnostics(c: &mut Criterion) {
    use oxify_connect_vision::{ErrorDiagnostic, VisionError};

    c.bench_function("diagnostic_model_load", |b| {
        b.iter(|| {
            let diag = ErrorDiagnostic::model_load(black_box("/path/to/model.onnx"));
            black_box(diag)
        })
    });

    c.bench_function("diagnostic_with_format", |b| {
        b.iter(|| {
            let err = VisionError::ModelNotLoaded;
            let formatted = err.with_diagnostics();
            black_box(formatted)
        })
    });
}

/// Benchmark configuration loading
fn bench_config(c: &mut Criterion) {
    use oxify_connect_vision::VisionConfig;

    let yaml_config = r#"
provider:
  name: mock
  use_gpu: false
cache:
  enabled: true
  max_entries: 1000
preprocessing:
  enabled: false
"#;

    c.bench_function("config_parse_yaml", |b| {
        b.iter(|| {
            let config = VisionConfig::from_yaml_str(black_box(yaml_config));
            black_box(config)
        })
    });

    let config = VisionConfig::default();
    c.bench_function("config_serialize_yaml", |b| {
        b.iter(|| {
            let yaml = serde_yaml::to_string(black_box(&config));
            black_box(yaml)
        })
    });

    c.bench_function("config_validate", |b| {
        b.iter(|| {
            let result = config.validate();
            black_box(result)
        })
    });
}

/// Memory profiling benchmark
fn bench_memory_usage(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_usage");
    group.sample_size(10); // Fewer samples for memory tests

    // Benchmark memory usage with different image sizes
    for size_kb in [10, 100, 1000] {
        let image = vec![0u8; size_kb * 1024];

        group.throughput(Throughput::Bytes((size_kb * 1024) as u64));
        group.bench_with_input(
            BenchmarkId::new("process_image", size_kb),
            &size_kb,
            |b, _| {
                let config = VisionProviderConfig::mock();
                let provider = create_provider(&config).unwrap();
                rt.block_on(provider.load_model()).unwrap();

                b.to_async(&rt).iter(|| async {
                    let result = provider.process_image(black_box(&image)).await;
                    black_box(result)
                })
            },
        );
    }

    group.finish();
}

/// Latency percentile benchmark
fn bench_latency_percentiles(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = VisionProviderConfig::mock();
    let provider = create_provider(&config).unwrap();
    rt.block_on(provider.load_model()).unwrap();

    let image = sample_image();

    let mut group = c.benchmark_group("latency_percentiles");
    group.sample_size(100); // More samples for better percentiles

    group.bench_function("ocr_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let result = provider.process_image(black_box(&image)).await;
            black_box(result)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_mock_provider,
    bench_preprocessing,
    bench_batch_processing,
    bench_cache,
    bench_downloader,
    bench_gpu_detection,
    bench_diagnostics,
    bench_config,
    bench_memory_usage,
    bench_latency_percentiles,
);

criterion_main!(benches);
