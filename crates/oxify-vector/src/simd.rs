//! SIMD-accelerated distance calculations
//!
//! This module provides SIMD-optimized implementations of distance metrics
//! for improved performance on supported CPUs.
//!
//! ## Features
//!
//! - **Auto-vectorization hints**: Helps the compiler generate SIMD code
//! - **x86_64 optimizations**: Automatically uses AVX-512, FMA+AVX2, or AVX2 when available
//! - **aarch64 optimizations**: Automatically uses NEON (always available on ARM64)
//! - **Cache-friendly**: Optimized memory access patterns
//! - **Fallback**: Automatically falls back to auto-vectorization on unsupported platforms
//!
//! ## Performance Hierarchy
//!
//! - **x86_64**: AVX-512 (16-wide) → FMA+AVX2 (8-wide) → AVX2 (8-wide) → auto-vectorization
//! - **aarch64**: NEON (4-wide)
//! - **other**: auto-vectorization
//!
//! ## Usage
//!
//! ```rust
//! use oxify_vector::simd::cosine_similarity_simd;
//!
//! let v1 = vec![1.0, 2.0, 3.0, 4.0];
//! let v2 = vec![2.0, 3.0, 4.0, 5.0];
//! let similarity = cosine_similarity_simd(&v1, &v2);
//! ```

// Allow unreachable code for architecture-specific optimizations
// On aarch64, NEON is always available so fallback code is never reached
// On x86_64 with AVX2, fallback code may not be reached
#![allow(unreachable_code)]

use crate::types::DistanceMetric;

// AVX2 intrinsics (x86_64 only)
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

// NEON intrinsics (aarch64 only)
#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

/// Check if AVX2 is available at runtime
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn is_avx2_available() -> bool {
    is_x86_feature_detected!("avx2")
}

/// Check if AVX2 is available at runtime (non-x86_64 always returns false)
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub fn is_avx2_available() -> bool {
    false
}

/// Check if FMA is available at runtime
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn is_fma_available() -> bool {
    is_x86_feature_detected!("fma")
}

/// Check if FMA is available at runtime (non-x86_64 always returns false)
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub fn is_fma_available() -> bool {
    false
}

/// Check if NEON is available at runtime (aarch64 always has NEON)
#[cfg(target_arch = "aarch64")]
#[inline]
pub fn is_neon_available() -> bool {
    // NEON is a mandatory feature on aarch64, always available
    true
}

/// Check if NEON is available at runtime (non-aarch64 always returns false)
#[cfg(not(target_arch = "aarch64"))]
#[inline]
pub fn is_neon_available() -> bool {
    false
}

/// Check if AVX-512 is available at runtime (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn is_avx512_available() -> bool {
    is_x86_feature_detected!("avx512f")
}

/// Check if AVX-512 is available at runtime (non-x86_64 always returns false)
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub fn is_avx512_available() -> bool {
    false
}

// ============================================================================
// AVX-512 Explicit Intrinsics (x86_64 only)
// ============================================================================

/// Horizontal sum of 16 f32 values in a 512-bit AVX-512 register
#[cfg(target_arch = "x86_64")]
#[inline]
unsafe fn horizontal_sum_avx512(v: __m512) -> f32 {
    // Extract high and low 256-bit lanes
    let low = _mm512_castps512_ps256(v); // Lower 8 elements
    let high = _mm512_extractf32x8_ps(v, 1); // Upper 8 elements

    // Add them together into a 256-bit vector
    let sum256 = _mm256_add_ps(low, high);

    // Use existing AVX2 horizontal sum
    horizontal_sum_avx2(sum256)
}

/// AVX-512 optimized dot product (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
#[inline]
unsafe fn dot_product_avx512(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm512_setzero_ps();

    // Process 16 floats at a time with AVX-512
    let chunks = len / 16;
    for i in 0..chunks {
        let offset = i * 16;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm512_loadu_ps(a_ptr);
        let b_vec = _mm512_loadu_ps(b_ptr);
        // FMA is part of AVX-512, so we can use it directly
        sum = _mm512_fmadd_ps(a_vec, b_vec, sum);
    }

    // Horizontal sum
    let mut total = horizontal_sum_avx512(sum);

    // Process remainder
    for i in (chunks * 16)..len {
        total += a[i] * b[i];
    }

    total
}

/// AVX-512 optimized cosine similarity (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
#[inline]
unsafe fn cosine_similarity_avx512(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut dot_sum = _mm512_setzero_ps();
    let mut norm_a_sum = _mm512_setzero_ps();
    let mut norm_b_sum = _mm512_setzero_ps();

    // Process 16 floats at a time
    let chunks = len / 16;
    for i in 0..chunks {
        let offset = i * 16;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm512_loadu_ps(a_ptr);
        let b_vec = _mm512_loadu_ps(b_ptr);

        // Use FMA for all three accumulations
        dot_sum = _mm512_fmadd_ps(a_vec, b_vec, dot_sum);
        norm_a_sum = _mm512_fmadd_ps(a_vec, a_vec, norm_a_sum);
        norm_b_sum = _mm512_fmadd_ps(b_vec, b_vec, norm_b_sum);
    }

    // Horizontal sum
    let mut dot = horizontal_sum_avx512(dot_sum);
    let mut norm_a = horizontal_sum_avx512(norm_a_sum);
    let mut norm_b = horizontal_sum_avx512(norm_b_sum);

    // Process remainder
    for i in (chunks * 16)..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denominator = (norm_a.sqrt() * norm_b.sqrt()).max(1e-10);
    dot / denominator
}

/// AVX-512 optimized Euclidean distance (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
#[inline]
unsafe fn euclidean_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum_sq = _mm512_setzero_ps();

    // Process 16 floats at a time
    let chunks = len / 16;
    for i in 0..chunks {
        let offset = i * 16;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm512_loadu_ps(a_ptr);
        let b_vec = _mm512_loadu_ps(b_ptr);
        let diff = _mm512_sub_ps(a_vec, b_vec);
        // FMA: sum_sq = diff * diff + sum_sq
        sum_sq = _mm512_fmadd_ps(diff, diff, sum_sq);
    }

    // Horizontal sum
    let mut total = horizontal_sum_avx512(sum_sq);

    // Process remainder
    for i in (chunks * 16)..len {
        let diff = a[i] - b[i];
        total += diff * diff;
    }

    total.sqrt()
}

/// AVX-512 optimized Manhattan distance (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
#[inline]
unsafe fn manhattan_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm512_setzero_ps();

    // Process 16 floats at a time
    let chunks = len / 16;
    for i in 0..chunks {
        let offset = i * 16;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm512_loadu_ps(a_ptr);
        let b_vec = _mm512_loadu_ps(b_ptr);
        let diff = _mm512_sub_ps(a_vec, b_vec);
        // abs(x) using built-in AVX-512 abs instruction
        let abs_diff = _mm512_abs_ps(diff);
        sum = _mm512_add_ps(sum, abs_diff);
    }

    // Horizontal sum
    let mut total = horizontal_sum_avx512(sum);

    // Process remainder
    for i in (chunks * 16)..len {
        total += (a[i] - b[i]).abs();
    }

    total
}

// ============================================================================
// ARM NEON Explicit Intrinsics (aarch64 only)
// ============================================================================

/// Horizontal sum of 4 f32 values in a 128-bit NEON register
#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn horizontal_sum_neon(v: float32x4_t) -> f32 {
    // Pairwise add: [a0, a1, a2, a3] -> [a0+a1, a2+a3, a0+a1, a2+a3]
    let pair_sum = vpaddq_f32(v, v);
    // Add pairs: [a0+a1, a2+a3, ...] -> [a0+a1+a2+a3, ...]
    let final_sum = vpaddq_f32(pair_sum, pair_sum);
    // Extract the result
    vgetq_lane_f32(final_sum, 0)
}

/// NEON-optimized dot product (aarch64 only)
#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn dot_product_neon(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = vdupq_n_f32(0.0);

    // Process 4 floats at a time with NEON
    let chunks = len / 4;
    for i in 0..chunks {
        let offset = i * 4;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = vld1q_f32(a_ptr);
        let b_vec = vld1q_f32(b_ptr);
        // Multiply and add: sum = a * b + sum
        sum = vmlaq_f32(sum, a_vec, b_vec);
    }

    // Horizontal sum
    let mut total = horizontal_sum_neon(sum);

    // Process remainder
    for i in (chunks * 4)..len {
        total += a[i] * b[i];
    }

    total
}

/// NEON-optimized cosine similarity (aarch64 only)
#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn cosine_similarity_neon(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut dot_sum = vdupq_n_f32(0.0);
    let mut norm_a_sum = vdupq_n_f32(0.0);
    let mut norm_b_sum = vdupq_n_f32(0.0);

    // Process 4 floats at a time
    let chunks = len / 4;
    for i in 0..chunks {
        let offset = i * 4;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = vld1q_f32(a_ptr);
        let b_vec = vld1q_f32(b_ptr);

        // Multiply and add
        dot_sum = vmlaq_f32(dot_sum, a_vec, b_vec);
        norm_a_sum = vmlaq_f32(norm_a_sum, a_vec, a_vec);
        norm_b_sum = vmlaq_f32(norm_b_sum, b_vec, b_vec);
    }

    // Horizontal sum
    let mut dot = horizontal_sum_neon(dot_sum);
    let mut norm_a = horizontal_sum_neon(norm_a_sum);
    let mut norm_b = horizontal_sum_neon(norm_b_sum);

    // Process remainder
    for i in (chunks * 4)..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denominator = (norm_a.sqrt() * norm_b.sqrt()).max(1e-10);
    dot / denominator
}

/// NEON-optimized Euclidean distance (aarch64 only)
#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn euclidean_distance_neon(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum_sq = vdupq_n_f32(0.0);

    // Process 4 floats at a time
    let chunks = len / 4;
    for i in 0..chunks {
        let offset = i * 4;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = vld1q_f32(a_ptr);
        let b_vec = vld1q_f32(b_ptr);
        let diff = vsubq_f32(a_vec, b_vec);
        // Multiply and add: sum_sq = diff * diff + sum_sq
        sum_sq = vmlaq_f32(sum_sq, diff, diff);
    }

    // Horizontal sum
    let mut total = horizontal_sum_neon(sum_sq);

    // Process remainder
    for i in (chunks * 4)..len {
        let diff = a[i] - b[i];
        total += diff * diff;
    }

    total.sqrt()
}

/// NEON-optimized Manhattan distance (aarch64 only)
#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn manhattan_distance_neon(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = vdupq_n_f32(0.0);

    // Process 4 floats at a time
    let chunks = len / 4;
    for i in 0..chunks {
        let offset = i * 4;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = vld1q_f32(a_ptr);
        let b_vec = vld1q_f32(b_ptr);
        let diff = vsubq_f32(a_vec, b_vec);
        let abs_diff = vabsq_f32(diff);
        sum = vaddq_f32(sum, abs_diff);
    }

    // Horizontal sum
    let mut total = horizontal_sum_neon(sum);

    // Process remainder
    for i in (chunks * 4)..len {
        total += (a[i] - b[i]).abs();
    }

    total
}

// ============================================================================
// AVX2 + FMA Explicit Intrinsics (x86_64 only)
// ============================================================================

/// FMA-optimized dot product (x86_64 only, requires FMA)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[inline]
unsafe fn dot_product_fma(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm256_setzero_ps();

    // Process 8 floats at a time with FMA
    let chunks = len / 8;
    for i in 0..chunks {
        let offset = i * 8;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm256_loadu_ps(a_ptr);
        let b_vec = _mm256_loadu_ps(b_ptr);
        // FMA: sum = a * b + sum (single instruction!)
        sum = _mm256_fmadd_ps(a_vec, b_vec, sum);
    }

    // Horizontal sum using optimized intrinsics
    let mut total = horizontal_sum_avx2(sum);

    // Process remainder
    for i in (chunks * 8)..len {
        total += a[i] * b[i];
    }

    total
}

/// FMA-optimized cosine similarity (x86_64 only, requires FMA)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[inline]
unsafe fn cosine_similarity_fma(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut dot_sum = _mm256_setzero_ps();
    let mut norm_a_sum = _mm256_setzero_ps();
    let mut norm_b_sum = _mm256_setzero_ps();

    // Process 8 floats at a time with FMA
    let chunks = len / 8;
    for i in 0..chunks {
        let offset = i * 8;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm256_loadu_ps(a_ptr);
        let b_vec = _mm256_loadu_ps(b_ptr);

        // Use FMA for all three accumulations
        dot_sum = _mm256_fmadd_ps(a_vec, b_vec, dot_sum);
        norm_a_sum = _mm256_fmadd_ps(a_vec, a_vec, norm_a_sum);
        norm_b_sum = _mm256_fmadd_ps(b_vec, b_vec, norm_b_sum);
    }

    // Horizontal sum
    let mut dot = horizontal_sum_avx2(dot_sum);
    let mut norm_a = horizontal_sum_avx2(norm_a_sum);
    let mut norm_b = horizontal_sum_avx2(norm_b_sum);

    // Process remainder
    for i in (chunks * 8)..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denominator = (norm_a.sqrt() * norm_b.sqrt()).max(1e-10);
    dot / denominator
}

/// FMA-optimized Euclidean distance (x86_64 only, requires FMA)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[inline]
unsafe fn euclidean_distance_fma(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum_sq = _mm256_setzero_ps();

    // Process 8 floats at a time with FMA
    let chunks = len / 8;
    for i in 0..chunks {
        let offset = i * 8;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm256_loadu_ps(a_ptr);
        let b_vec = _mm256_loadu_ps(b_ptr);
        let diff = _mm256_sub_ps(a_vec, b_vec);
        // FMA: sum_sq = diff * diff + sum_sq
        sum_sq = _mm256_fmadd_ps(diff, diff, sum_sq);
    }

    // Horizontal sum
    let mut total = horizontal_sum_avx2(sum_sq);

    // Process remainder
    for i in (chunks * 8)..len {
        let diff = a[i] - b[i];
        total += diff * diff;
    }

    total.sqrt()
}

/// Horizontal sum of 8 f32 values in a 256-bit register (AVX2 helper)
#[cfg(target_arch = "x86_64")]
#[inline]
unsafe fn horizontal_sum_avx2(v: __m256) -> f32 {
    // v = [a0, a1, a2, a3, a4, a5, a6, a7]
    // Extract high and low 128-bit lanes
    let hi = _mm256_extractf128_ps(v, 1); // [a4, a5, a6, a7]
    let lo = _mm256_castps256_ps128(v); // [a0, a1, a2, a3]

    // Add high and low lanes
    let sum128 = _mm_add_ps(lo, hi); // [a0+a4, a1+a5, a2+a6, a3+a7]

    // Horizontal add twice to sum all 4 elements
    let sum64 = _mm_hadd_ps(sum128, sum128); // [a0+a4+a1+a5, a2+a6+a3+a7, ...]
    let sum32 = _mm_hadd_ps(sum64, sum64); // [sum_all, sum_all, ...]

    // Extract the final sum
    _mm_cvtss_f32(sum32)
}

/// AVX2-optimized dot product (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm256_setzero_ps();

    // Process 8 floats at a time with AVX2
    let chunks = len / 8;
    for i in 0..chunks {
        let offset = i * 8;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm256_loadu_ps(a_ptr);
        let b_vec = _mm256_loadu_ps(b_ptr);
        let mul = _mm256_mul_ps(a_vec, b_vec);
        sum = _mm256_add_ps(sum, mul);
    }

    // Horizontal sum of 8 floats using optimized intrinsics
    let mut total = horizontal_sum_avx2(sum);

    // Process remainder
    for i in (chunks * 8)..len {
        total += a[i] * b[i];
    }

    total
}

/// AVX2-optimized cosine similarity (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn cosine_similarity_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut dot_sum = _mm256_setzero_ps();
    let mut norm_a_sum = _mm256_setzero_ps();
    let mut norm_b_sum = _mm256_setzero_ps();

    // Process 8 floats at a time
    let chunks = len / 8;
    for i in 0..chunks {
        let offset = i * 8;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm256_loadu_ps(a_ptr);
        let b_vec = _mm256_loadu_ps(b_ptr);

        dot_sum = _mm256_add_ps(dot_sum, _mm256_mul_ps(a_vec, b_vec));
        norm_a_sum = _mm256_add_ps(norm_a_sum, _mm256_mul_ps(a_vec, a_vec));
        norm_b_sum = _mm256_add_ps(norm_b_sum, _mm256_mul_ps(b_vec, b_vec));
    }

    // Horizontal sum using optimized intrinsics
    let mut dot = horizontal_sum_avx2(dot_sum);
    let mut norm_a = horizontal_sum_avx2(norm_a_sum);
    let mut norm_b = horizontal_sum_avx2(norm_b_sum);

    // Process remainder
    for i in (chunks * 8)..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denominator = (norm_a.sqrt() * norm_b.sqrt()).max(1e-10);
    dot / denominator
}

/// AVX2-optimized Euclidean distance (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn euclidean_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum_sq = _mm256_setzero_ps();

    // Process 8 floats at a time
    let chunks = len / 8;
    for i in 0..chunks {
        let offset = i * 8;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm256_loadu_ps(a_ptr);
        let b_vec = _mm256_loadu_ps(b_ptr);
        let diff = _mm256_sub_ps(a_vec, b_vec);
        sum_sq = _mm256_add_ps(sum_sq, _mm256_mul_ps(diff, diff));
    }

    // Horizontal sum using optimized intrinsics
    let mut total = horizontal_sum_avx2(sum_sq);

    // Process remainder
    for i in (chunks * 8)..len {
        let diff = a[i] - b[i];
        total += diff * diff;
    }

    total.sqrt()
}

/// AVX2-optimized Manhattan distance (x86_64 only)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn manhattan_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm256_setzero_ps();
    let sign_mask = _mm256_set1_ps(-0.0); // Mask for abs

    // Process 8 floats at a time
    let chunks = len / 8;
    for i in 0..chunks {
        let offset = i * 8;
        let a_ptr = a.as_ptr().add(offset);
        let b_ptr = b.as_ptr().add(offset);

        let a_vec = _mm256_loadu_ps(a_ptr);
        let b_vec = _mm256_loadu_ps(b_ptr);
        let diff = _mm256_sub_ps(a_vec, b_vec);
        // abs(x) = andnot(sign_bit, x)
        let abs_diff = _mm256_andnot_ps(sign_mask, diff);
        sum = _mm256_add_ps(sum, abs_diff);
    }

    // Horizontal sum using optimized intrinsics
    let mut total = horizontal_sum_avx2(sum);

    // Process remainder
    for i in (chunks * 8)..len {
        total += (a[i] - b[i]).abs();
    }

    total
}

// ============================================================================
// Auto-Vectorization Fallback Implementations
// These functions are kept for testing and as fallbacks on platforms without SIMD
// ============================================================================

/// Auto-vectorization fallback for cosine similarity
#[inline]
#[allow(dead_code)]
fn cosine_similarity_autovec(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    // Use chunks for better vectorization
    let chunk_size = 8; // Process 8 elements at a time for better SIMD utilization
    let len = a.len();
    let chunks = len / chunk_size;

    let mut dot_product = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    // Process chunks (compiler will auto-vectorize this)
    for i in 0..chunks {
        let offset = i * chunk_size;
        for j in 0..chunk_size {
            let idx = offset + j;
            let a_val = unsafe { *a.get_unchecked(idx) };
            let b_val = unsafe { *b.get_unchecked(idx) };

            dot_product += a_val * b_val;
            norm_a += a_val * a_val;
            norm_b += b_val * b_val;
        }
    }

    // Process remainder
    for i in (chunks * chunk_size)..len {
        let a_val = unsafe { *a.get_unchecked(i) };
        let b_val = unsafe { *b.get_unchecked(i) };

        dot_product += a_val * b_val;
        norm_a += a_val * a_val;
        norm_b += b_val * b_val;
    }

    let denominator = (norm_a.sqrt() * norm_b.sqrt()).max(1e-10);
    dot_product / denominator
}

/// SIMD-optimized cosine similarity calculation
///
/// Automatically uses the best available SIMD implementation:
/// - x86_64: AVX-512 → FMA+AVX2 → AVX2 → auto-vectorization
/// - aarch64: NEON (always available)
/// - other: auto-vectorization
#[inline]
pub fn cosine_similarity_simd(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_avx512_available() {
            unsafe { cosine_similarity_avx512(a, b) }
        } else if is_fma_available() {
            unsafe { cosine_similarity_fma(a, b) }
        } else if is_avx2_available() {
            unsafe { cosine_similarity_avx2(a, b) }
        } else {
            cosine_similarity_autovec(a, b)
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        unsafe { cosine_similarity_neon(a, b) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        cosine_similarity_autovec(a, b)
    }
}

/// Auto-vectorization fallback for Euclidean distance
#[inline]
#[allow(dead_code)]
fn euclidean_distance_autovec(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    let chunk_size = 8;
    let len = a.len();
    let chunks = len / chunk_size;

    let mut sum_sq = 0.0f32;

    // Process chunks (compiler will auto-vectorize this)
    for i in 0..chunks {
        let offset = i * chunk_size;
        for j in 0..chunk_size {
            let idx = offset + j;
            let diff = unsafe { *a.get_unchecked(idx) - *b.get_unchecked(idx) };
            sum_sq += diff * diff;
        }
    }

    // Process remainder
    for i in (chunks * chunk_size)..len {
        let diff = unsafe { *a.get_unchecked(i) - *b.get_unchecked(i) };
        sum_sq += diff * diff;
    }

    sum_sq.sqrt()
}

/// SIMD-optimized Euclidean distance calculation
///
/// Automatically uses the best available SIMD implementation:
/// - x86_64: AVX-512 → FMA+AVX2 → AVX2 → auto-vectorization
/// - aarch64: NEON (always available)
/// - other: auto-vectorization
#[inline]
pub fn euclidean_distance_simd(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_avx512_available() {
            unsafe { euclidean_distance_avx512(a, b) }
        } else if is_fma_available() {
            unsafe { euclidean_distance_fma(a, b) }
        } else if is_avx2_available() {
            unsafe { euclidean_distance_avx2(a, b) }
        } else {
            euclidean_distance_autovec(a, b)
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        unsafe { euclidean_distance_neon(a, b) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        euclidean_distance_autovec(a, b)
    }
}

/// Auto-vectorization fallback for dot product
#[inline]
#[allow(dead_code)]
fn dot_product_autovec(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    let chunk_size = 8;
    let len = a.len();
    let chunks = len / chunk_size;

    let mut dot = 0.0f32;

    // Process chunks (compiler will auto-vectorize this)
    for i in 0..chunks {
        let offset = i * chunk_size;
        for j in 0..chunk_size {
            let idx = offset + j;
            dot += unsafe { *a.get_unchecked(idx) * *b.get_unchecked(idx) };
        }
    }

    // Process remainder
    for i in (chunks * chunk_size)..len {
        dot += unsafe { *a.get_unchecked(i) * *b.get_unchecked(i) };
    }

    dot
}

/// SIMD-optimized dot product calculation
///
/// Automatically uses the best available SIMD implementation:
/// - x86_64: AVX-512 → FMA+AVX2 → AVX2 → auto-vectorization
/// - aarch64: NEON (always available)
/// - other: auto-vectorization
#[inline]
pub fn dot_product_simd(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_avx512_available() {
            unsafe { dot_product_avx512(a, b) }
        } else if is_fma_available() {
            unsafe { dot_product_fma(a, b) }
        } else if is_avx2_available() {
            unsafe { dot_product_avx2(a, b) }
        } else {
            dot_product_autovec(a, b)
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        unsafe { dot_product_neon(a, b) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        dot_product_autovec(a, b)
    }
}

/// Auto-vectorization fallback for Manhattan distance
#[inline]
#[allow(dead_code)]
fn manhattan_distance_autovec(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    let chunk_size = 8;
    let len = a.len();
    let chunks = len / chunk_size;

    let mut sum = 0.0f32;

    // Process chunks (compiler will auto-vectorize this)
    for i in 0..chunks {
        let offset = i * chunk_size;
        for j in 0..chunk_size {
            let idx = offset + j;
            sum += unsafe { (*a.get_unchecked(idx) - *b.get_unchecked(idx)).abs() };
        }
    }

    // Process remainder
    for i in (chunks * chunk_size)..len {
        sum += unsafe { (*a.get_unchecked(i) - *b.get_unchecked(i)).abs() };
    }

    sum
}

/// SIMD-optimized Manhattan distance calculation
///
/// Automatically uses the best available SIMD implementation:
/// - x86_64: AVX-512 → AVX2 → auto-vectorization
/// - aarch64: NEON (always available)
/// - other: auto-vectorization
#[inline]
pub fn manhattan_distance_simd(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_avx512_available() {
            unsafe { manhattan_distance_avx512(a, b) }
        } else if is_avx2_available() {
            unsafe { manhattan_distance_avx2(a, b) }
        } else {
            manhattan_distance_autovec(a, b)
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        unsafe { manhattan_distance_neon(a, b) }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        manhattan_distance_autovec(a, b)
    }
}

/// Compute similarity/distance using the specified metric with SIMD optimization
///
/// Returns a score where higher is better (for VectorSearchIndex).
pub fn compute_distance_simd(metric: DistanceMetric, a: &[f32], b: &[f32]) -> f32 {
    match metric {
        DistanceMetric::Cosine => cosine_similarity_simd(a, b),
        DistanceMetric::Euclidean => -euclidean_distance_simd(a, b),
        DistanceMetric::DotProduct => dot_product_simd(a, b),
        DistanceMetric::Manhattan => -manhattan_distance_simd(a, b),
    }
}

/// Compute distance using the specified metric with SIMD optimization
///
/// Returns a distance where lower is better (for HNSW and other ANN algorithms).
#[inline]
pub fn compute_distance_lower_is_better_simd(metric: DistanceMetric, a: &[f32], b: &[f32]) -> f32 {
    match metric {
        DistanceMetric::Cosine => {
            // 1 - cosine similarity = cosine distance
            1.0 - cosine_similarity_simd(a, b)
        }
        DistanceMetric::Euclidean => euclidean_distance_simd(a, b),
        DistanceMetric::DotProduct => {
            // Negative dot product (lower is better)
            -dot_product_simd(a, b)
        }
        DistanceMetric::Manhattan => manhattan_distance_simd(a, b),
    }
}

// ============================================================================
// Quantized Vector Distance (u8/int8) - SIMD Optimized
// ============================================================================

/// Compute Manhattan distance between two quantized (u8) vectors using SIMD
///
/// This is significantly faster than converting to f32 and using regular distance.
/// Optimized for scalar quantization (8-bit).
#[inline]
pub fn quantized_manhattan_distance_simd(a: &[u8], b: &[u8]) -> u32 {
    assert_eq!(a.len(), b.len(), "Vector dimension mismatch");

    #[cfg(target_arch = "x86_64")]
    {
        if is_avx2_available() {
            return unsafe { quantized_manhattan_distance_avx2(a, b) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { quantized_manhattan_distance_neon(a, b) };
    }

    // Fallback: scalar implementation
    quantized_manhattan_distance_scalar(a, b)
}

/// Compute dot product between two quantized (u8) vectors using SIMD
///
/// Returns u32 to avoid overflow (max value = 255*255*len).
/// For normalized comparison, you may need to convert to f32 afterward.
#[inline]
pub fn quantized_dot_product_simd(a: &[u8], b: &[u8]) -> u32 {
    assert_eq!(a.len(), b.len(), "Vector dimension mismatch");

    #[cfg(target_arch = "x86_64")]
    {
        if is_avx2_available() {
            return unsafe { quantized_dot_product_avx2(a, b) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { quantized_dot_product_neon(a, b) };
    }

    // Fallback: scalar implementation
    quantized_dot_product_scalar(a, b)
}

/// Compute Euclidean distance between two quantized (u8) vectors using SIMD
///
/// Returns the squared distance to avoid sqrt overhead.
/// If you need actual distance, take sqrt of the result.
#[inline]
pub fn quantized_euclidean_squared_simd(a: &[u8], b: &[u8]) -> u32 {
    assert_eq!(a.len(), b.len(), "Vector dimension mismatch");

    #[cfg(target_arch = "x86_64")]
    {
        if is_avx2_available() {
            return unsafe { quantized_euclidean_squared_avx2(a, b) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { quantized_euclidean_squared_neon(a, b) };
    }

    // Fallback: scalar implementation
    quantized_euclidean_squared_scalar(a, b)
}

// ============================================================================
// Scalar implementations (fallback)
// ============================================================================

#[inline]
fn quantized_manhattan_distance_scalar(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x as i32 - y as i32).unsigned_abs())
        .sum()
}

#[inline]
fn quantized_dot_product_scalar(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| x as u32 * y as u32)
        .sum()
}

#[inline]
fn quantized_euclidean_squared_scalar(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let diff = x as i32 - y as i32;
            (diff * diff) as u32
        })
        .sum()
}

// ============================================================================
// AVX2 implementations (x86_64)
// ============================================================================

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn quantized_manhattan_distance_avx2(a: &[u8], b: &[u8]) -> u32 {
    let len = a.len();
    let mut sum = _mm256_setzero_si256();

    let mut i = 0;
    // Process 32 bytes at a time with AVX2
    while i + 32 <= len {
        let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
        let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);

        // Compute absolute difference using unsigned saturation trick
        let diff1 = _mm256_subs_epu8(va, vb);
        let diff2 = _mm256_subs_epu8(vb, va);
        let abs_diff = _mm256_or_si256(diff1, diff2);

        // Extend to 16-bit to avoid overflow in horizontal sum
        let abs_diff_lo = _mm256_unpacklo_epi8(abs_diff, _mm256_setzero_si256());
        let abs_diff_hi = _mm256_unpackhi_epi8(abs_diff, _mm256_setzero_si256());

        // Add to accumulator
        sum = _mm256_add_epi16(sum, abs_diff_lo);
        sum = _mm256_add_epi16(sum, abs_diff_hi);

        i += 32;
    }

    // Horizontal sum of 16-bit values
    let sum_lo = _mm256_unpacklo_epi16(sum, _mm256_setzero_si256());
    let sum_hi = _mm256_unpackhi_epi16(sum, _mm256_setzero_si256());
    let sum32 = _mm256_add_epi32(sum_lo, sum_hi);

    // Extract and sum all lanes
    let mut result_arr = [0u32; 8];
    _mm256_storeu_si256(result_arr.as_mut_ptr() as *mut __m256i, sum32);
    let mut result: u32 = result_arr.iter().sum();

    // Handle remaining elements
    while i < len {
        result += (a[i] as i32 - b[i] as i32).unsigned_abs();
        i += 1;
    }

    result
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn quantized_dot_product_avx2(a: &[u8], b: &[u8]) -> u32 {
    let len = a.len();
    let mut sum = _mm256_setzero_si256();

    let mut i = 0;
    // Process 16 bytes at a time (to fit in 16-bit accumulators)
    while i + 16 <= len {
        // Load 16 bytes from each vector
        let va_128 = _mm_loadu_si128(a.as_ptr().add(i) as *const __m128i);
        let vb_128 = _mm_loadu_si128(b.as_ptr().add(i) as *const __m128i);

        // Extend to 256-bit
        let va = _mm256_cvtepu8_epi16(va_128);
        let vb = _mm256_cvtepu8_epi16(vb_128);

        // Multiply: 16-bit * 16-bit = 32-bit (using _mm256_madd_epi16)
        let prod = _mm256_madd_epi16(va, vb);
        sum = _mm256_add_epi32(sum, prod);

        i += 16;
    }

    // Extract and sum all lanes
    let mut result_arr = [0u32; 8];
    _mm256_storeu_si256(result_arr.as_mut_ptr() as *mut __m256i, sum);
    let mut result: u32 = result_arr.iter().sum();

    // Handle remaining elements
    while i < len {
        result += a[i] as u32 * b[i] as u32;
        i += 1;
    }

    result
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn quantized_euclidean_squared_avx2(a: &[u8], b: &[u8]) -> u32 {
    let len = a.len();
    let mut sum = _mm256_setzero_si256();

    let mut i = 0;
    // Process 16 bytes at a time (to fit in 16-bit intermediate results)
    while i + 16 <= len {
        let va_128 = _mm_loadu_si128(a.as_ptr().add(i) as *const __m128i);
        let vb_128 = _mm_loadu_si128(b.as_ptr().add(i) as *const __m128i);

        // Extend to 16-bit signed
        let va = _mm256_cvtepu8_epi16(va_128);
        let vb = _mm256_cvtepu8_epi16(vb_128);

        // Compute difference
        let diff = _mm256_sub_epi16(va, vb);

        // Square using _mm256_madd_epi16 (diff * diff)
        let squared = _mm256_madd_epi16(diff, diff);
        sum = _mm256_add_epi32(sum, squared);

        i += 16;
    }

    // Extract and sum all lanes
    let mut result_arr = [0u32; 8];
    _mm256_storeu_si256(result_arr.as_mut_ptr() as *mut __m256i, sum);
    let mut result: u32 = result_arr.iter().sum();

    // Handle remaining elements
    while i < len {
        let diff = a[i] as i32 - b[i] as i32;
        result += (diff * diff) as u32;
        i += 1;
    }

    result
}

// ============================================================================
// NEON implementations (aarch64)
// ============================================================================

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn quantized_manhattan_distance_neon(a: &[u8], b: &[u8]) -> u32 {
    let len = a.len();
    let mut sum = vdupq_n_u32(0);

    let mut i = 0;
    // Process 16 bytes at a time
    while i + 16 <= len {
        let va = vld1q_u8(a.as_ptr().add(i));
        let vb = vld1q_u8(b.as_ptr().add(i));

        // Compute absolute difference
        let abs_diff = vabdq_u8(va, vb);

        // Extend to 16-bit and accumulate
        let abs_diff_lo = vmovl_u8(vget_low_u8(abs_diff));
        let abs_diff_hi = vmovl_u8(vget_high_u8(abs_diff));

        // Accumulate into 32-bit
        sum = vaddw_u16(sum, vget_low_u16(abs_diff_lo));
        sum = vaddw_u16(sum, vget_high_u16(abs_diff_lo));
        sum = vaddw_u16(sum, vget_low_u16(abs_diff_hi));
        sum = vaddw_u16(sum, vget_high_u16(abs_diff_hi));

        i += 16;
    }

    // Horizontal sum
    let mut result = vaddvq_u32(sum);

    // Handle remaining elements
    while i < len {
        result += (a[i] as i32 - b[i] as i32).unsigned_abs();
        i += 1;
    }

    result
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn quantized_dot_product_neon(a: &[u8], b: &[u8]) -> u32 {
    let len = a.len();
    let mut sum = vdupq_n_u32(0);

    let mut i = 0;
    // Process 8 bytes at a time (to avoid overflow)
    while i + 8 <= len {
        let va = vld1_u8(a.as_ptr().add(i));
        let vb = vld1_u8(b.as_ptr().add(i));

        // Extend to 16-bit
        let va_16 = vmovl_u8(va);
        let vb_16 = vmovl_u8(vb);

        // Multiply and accumulate
        let prod = vmull_u16(vget_low_u16(va_16), vget_low_u16(vb_16));
        sum = vaddq_u32(sum, prod);

        let prod_hi = vmull_u16(vget_high_u16(va_16), vget_high_u16(vb_16));
        sum = vaddq_u32(sum, prod_hi);

        i += 8;
    }

    // Horizontal sum
    let mut result = vaddvq_u32(sum);

    // Handle remaining elements
    while i < len {
        result += a[i] as u32 * b[i] as u32;
        i += 1;
    }

    result
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn quantized_euclidean_squared_neon(a: &[u8], b: &[u8]) -> u32 {
    let len = a.len();
    let mut sum = vdupq_n_u32(0);

    let mut i = 0;
    // Process 8 bytes at a time
    while i + 8 <= len {
        let va = vld1_u8(a.as_ptr().add(i));
        let vb = vld1_u8(b.as_ptr().add(i));

        // Compute absolute difference and extend to 16-bit
        let abs_diff = vabd_u8(va, vb);
        let diff_16 = vmovl_u8(abs_diff);

        // Square and accumulate
        let squared = vmull_u16(vget_low_u16(diff_16), vget_low_u16(diff_16));
        sum = vaddq_u32(sum, squared);

        let squared_hi = vmull_u16(vget_high_u16(diff_16), vget_high_u16(diff_16));
        sum = vaddq_u32(sum, squared_hi);

        i += 8;
    }

    // Horizontal sum
    let mut result = vaddvq_u32(sum);

    // Handle remaining elements
    while i < len {
        let diff = a[i] as i32 - b[i] as i32;
        result += (diff * diff) as u32;
        i += 1;
    }

    result
}

// ============================================================================
// Vector Normalization (SIMD-optimized)
// ============================================================================

/// Normalize a vector in-place using SIMD optimization
///
/// Computes the L2 norm and divides each element by it.
/// Uses SIMD-optimized dot product for norm calculation.
#[inline]
pub fn normalize_vector_simd(vec: &mut [f32]) {
    // Compute L2 norm using SIMD dot product
    let norm_squared = dot_product_simd(vec, vec);
    let norm = norm_squared.sqrt();

    if norm > 1e-10 {
        let inv_norm = 1.0 / norm;
        scale_vector_simd(vec, inv_norm);
    }
}

/// Scale a vector by a constant using SIMD optimization
///
/// Multiplies each element by the given scalar.
#[inline]
pub fn scale_vector_simd(vec: &mut [f32], scalar: f32) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_avx512_available() {
            unsafe {
                scale_vector_avx512(vec, scalar);
            }
            return;
        }
        if is_avx2_available() {
            unsafe {
                scale_vector_avx2(vec, scalar);
            }
            return;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        unsafe {
            scale_vector_neon(vec, scalar);
        }
        return;
    }

    // Fallback to scalar implementation with auto-vectorization hints
    for x in vec.iter_mut() {
        *x *= scalar;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
#[inline]
unsafe fn scale_vector_avx512(vec: &mut [f32], scalar: f32) {
    let len = vec.len();
    let scalar_vec = _mm512_set1_ps(scalar);
    let mut i = 0;

    // Process 16 floats at a time
    while i + 16 <= len {
        let ptr = vec.as_mut_ptr().add(i);
        let v = _mm512_loadu_ps(ptr);
        let scaled = _mm512_mul_ps(v, scalar_vec);
        _mm512_storeu_ps(ptr, scaled);
        i += 16;
    }

    // Handle remainder
    while i < len {
        vec[i] *= scalar;
        i += 1;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn scale_vector_avx2(vec: &mut [f32], scalar: f32) {
    let len = vec.len();
    let scalar_vec = _mm256_set1_ps(scalar);
    let mut i = 0;

    // Process 8 floats at a time
    while i + 8 <= len {
        let ptr = vec.as_mut_ptr().add(i);
        let v = _mm256_loadu_ps(ptr);
        let scaled = _mm256_mul_ps(v, scalar_vec);
        _mm256_storeu_ps(ptr, scaled);
        i += 8;
    }

    // Handle remainder
    while i < len {
        vec[i] *= scalar;
        i += 1;
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn scale_vector_neon(vec: &mut [f32], scalar: f32) {
    let len = vec.len();
    let scalar_vec = vdupq_n_f32(scalar);
    let mut i = 0;

    // Process 4 floats at a time
    while i + 4 <= len {
        let ptr = vec.as_mut_ptr().add(i);
        let v = vld1q_f32(ptr);
        let scaled = vmulq_f32(v, scalar_vec);
        vst1q_f32(ptr, scaled);
        i += 4;
    }

    // Handle remainder
    while i < len {
        vec[i] *= scalar;
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_simd() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity_simd(&v1, &v2);
        assert!((sim - 1.0).abs() < 1e-6);

        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity_simd(&v1, &v2);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_simd_large() {
        // Test with vectors larger than chunk size
        let v1: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let v2: Vec<f32> = (0..100).map(|i| (i + 1) as f32).collect();
        let sim = cosine_similarity_simd(&v1, &v2);
        assert!(sim > 0.99); // Highly correlated
    }

    #[test]
    fn test_euclidean_distance_simd() {
        let v1 = vec![0.0, 0.0, 0.0];
        let v2 = vec![3.0, 4.0, 0.0];
        let dist = euclidean_distance_simd(&v1, &v2);
        assert!((dist - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_distance_simd_large() {
        let v1 = vec![0.0; 100];
        let v2 = vec![1.0; 100];
        let dist = euclidean_distance_simd(&v1, &v2);
        assert!((dist - 10.0).abs() < 1e-6); // sqrt(100)
    }

    #[test]
    fn test_dot_product_simd() {
        let v1 = vec![1.0, 2.0, 3.0];
        let v2 = vec![4.0, 5.0, 6.0];
        let dot = dot_product_simd(&v1, &v2);
        assert!((dot - 32.0).abs() < 1e-6); // 1*4 + 2*5 + 3*6 = 32
    }

    #[test]
    fn test_dot_product_simd_large() {
        let v1: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        let v2: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        let dot = dot_product_simd(&v1, &v2);
        let expected: f32 = (1..=100).map(|i| (i * i) as f32).sum();
        assert!((dot - expected).abs() < 1e-3);
    }

    #[test]
    fn test_manhattan_distance_simd() {
        let v1 = vec![1.0, 2.0, 3.0];
        let v2 = vec![4.0, 5.0, 6.0];
        let dist = manhattan_distance_simd(&v1, &v2);
        assert!((dist - 9.0).abs() < 1e-6); // |1-4| + |2-5| + |3-6| = 9
    }

    #[test]
    fn test_manhattan_distance_simd_large() {
        let v1 = vec![0.0; 100];
        let v2 = vec![1.0; 100];
        let dist = manhattan_distance_simd(&v1, &v2);
        assert!((dist - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_distance_simd() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];

        let sim = compute_distance_simd(DistanceMetric::Cosine, &v1, &v2);
        assert!((sim - 1.0).abs() < 1e-6);

        let dist = compute_distance_simd(DistanceMetric::Euclidean, &v1, &v2);
        assert!(dist.abs() < 1e-6); // Distance is 0, but returned as -0.0

        let dot = compute_distance_simd(DistanceMetric::DotProduct, &v1, &v2);
        assert!((dot - 1.0).abs() < 1e-6);

        let manhattan = compute_distance_simd(DistanceMetric::Manhattan, &v1, &v2);
        assert!(manhattan.abs() < 1e-6);
    }

    #[test]
    fn test_is_avx2_available() {
        // Test that the function doesn't panic
        let _available = is_avx2_available();
        // On x86_64, it should detect AVX2 support (or not)
        // On other architectures, it should always return false
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!is_avx2_available());
    }

    #[test]
    fn test_is_neon_available() {
        // Test that the function doesn't panic
        let available = is_neon_available();
        // On aarch64, NEON is always available (mandatory feature)
        #[cfg(target_arch = "aarch64")]
        assert!(available, "NEON should always be available on aarch64");
        // On other architectures, it should always return false
        #[cfg(not(target_arch = "aarch64"))]
        assert!(!available, "NEON should not be available on non-aarch64");
    }

    #[test]
    fn test_is_avx512_available() {
        // Test that the function doesn't panic
        let _available = is_avx512_available();
        // On x86_64, it should detect AVX-512 support (or not)
        // On other architectures, it should always return false
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!is_avx512_available());
    }

    #[test]
    fn test_avx2_correctness() {
        // Test that AVX2 implementations give same results as auto-vectorized ones
        let v1: Vec<f32> = (0..768).map(|i| (i as f32) * 0.01).collect();
        let v2: Vec<f32> = (0..768).map(|i| (i as f32) * 0.02).collect();

        let cosine = cosine_similarity_simd(&v1, &v2);
        let euclidean = euclidean_distance_simd(&v1, &v2);
        let dot = dot_product_simd(&v1, &v2);
        let manhattan = manhattan_distance_simd(&v1, &v2);

        // Verify results are reasonable
        assert!(cosine > 0.0 && cosine <= 1.0);
        assert!(euclidean > 0.0);
        assert!(dot > 0.0);
        assert!(manhattan > 0.0);

        // Compare with autovec versions
        let cosine_autovec = cosine_similarity_autovec(&v1, &v2);
        let euclidean_autovec = euclidean_distance_autovec(&v1, &v2);
        let dot_autovec = dot_product_autovec(&v1, &v2);
        let manhattan_autovec = manhattan_distance_autovec(&v1, &v2);

        // Use relative error for large values
        let relative_error = |a: f32, b: f32| (a - b).abs() / a.max(b).max(1.0);
        assert!(relative_error(cosine, cosine_autovec) < 1e-5);
        assert!(relative_error(euclidean, euclidean_autovec) < 1e-5);
        assert!(relative_error(dot, dot_autovec) < 1e-5);
        assert!(relative_error(manhattan, manhattan_autovec) < 1e-5);
    }

    #[test]
    fn test_neon_correctness() {
        // Test that NEON implementations give same results as auto-vectorized ones
        let v1: Vec<f32> = (0..768).map(|i| (i as f32) * 0.01).collect();
        let v2: Vec<f32> = (0..768).map(|i| (i as f32) * 0.02).collect();

        let cosine = cosine_similarity_simd(&v1, &v2);
        let euclidean = euclidean_distance_simd(&v1, &v2);
        let dot = dot_product_simd(&v1, &v2);
        let manhattan = manhattan_distance_simd(&v1, &v2);

        // Verify results are reasonable
        assert!(cosine > 0.0 && cosine <= 1.0);
        assert!(euclidean > 0.0);
        assert!(dot > 0.0);
        assert!(manhattan > 0.0);

        // Compare with autovec versions
        let cosine_autovec = cosine_similarity_autovec(&v1, &v2);
        let euclidean_autovec = euclidean_distance_autovec(&v1, &v2);
        let dot_autovec = dot_product_autovec(&v1, &v2);
        let manhattan_autovec = manhattan_distance_autovec(&v1, &v2);

        // Use relative error for large values
        let relative_error = |a: f32, b: f32| (a - b).abs() / a.max(b).max(1.0);
        assert!(relative_error(cosine, cosine_autovec) < 1e-5);
        assert!(relative_error(euclidean, euclidean_autovec) < 1e-5);
        assert!(relative_error(dot, dot_autovec) < 1e-5);
        assert!(relative_error(manhattan, manhattan_autovec) < 1e-5);
    }

    #[test]
    fn test_avx512_correctness() {
        // Test that AVX-512 implementations give same results as auto-vectorized ones
        let v1: Vec<f32> = (0..1024).map(|i| (i as f32) * 0.01).collect();
        let v2: Vec<f32> = (0..1024).map(|i| (i as f32) * 0.02).collect();

        let cosine = cosine_similarity_simd(&v1, &v2);
        let euclidean = euclidean_distance_simd(&v1, &v2);
        let dot = dot_product_simd(&v1, &v2);
        let manhattan = manhattan_distance_simd(&v1, &v2);

        // Verify results are reasonable
        assert!(cosine > 0.0 && cosine <= 1.0);
        assert!(euclidean > 0.0);
        assert!(dot > 0.0);
        assert!(manhattan > 0.0);

        // Compare with autovec versions
        let cosine_autovec = cosine_similarity_autovec(&v1, &v2);
        let euclidean_autovec = euclidean_distance_autovec(&v1, &v2);
        let dot_autovec = dot_product_autovec(&v1, &v2);
        let manhattan_autovec = manhattan_distance_autovec(&v1, &v2);

        // Use relative error for large values
        let relative_error = |a: f32, b: f32| (a - b).abs() / a.max(b).max(1.0);
        assert!(relative_error(cosine, cosine_autovec) < 1e-5);
        assert!(relative_error(euclidean, euclidean_autovec) < 1e-5);
        assert!(relative_error(dot, dot_autovec) < 1e-5);
        assert!(relative_error(manhattan, manhattan_autovec) < 1e-5);
    }

    #[test]
    fn test_quantized_manhattan_distance() {
        // Test quantized Manhattan distance
        let a = vec![10u8, 20, 30, 40, 50, 60, 70, 80];
        let b = vec![15u8, 25, 35, 45, 55, 65, 75, 85];

        let distance_simd = quantized_manhattan_distance_simd(&a, &b);
        let distance_scalar = quantized_manhattan_distance_scalar(&a, &b);

        assert_eq!(distance_simd, distance_scalar);
        assert_eq!(distance_simd, 40); // |10-15| + |20-25| + ... = 5*8 = 40
    }

    #[test]
    fn test_quantized_manhattan_distance_large() {
        // Test with larger vectors (768 dimensions)
        let a: Vec<u8> = (0..768).map(|i| (i % 256) as u8).collect();
        let b: Vec<u8> = (0..768).map(|i| ((i + 10) % 256) as u8).collect();

        let distance_simd = quantized_manhattan_distance_simd(&a, &b);
        let distance_scalar = quantized_manhattan_distance_scalar(&a, &b);

        assert_eq!(distance_simd, distance_scalar);
    }

    #[test]
    fn test_quantized_dot_product() {
        // Test quantized dot product
        let a = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let b = vec![8u8, 7, 6, 5, 4, 3, 2, 1];

        let dot_simd = quantized_dot_product_simd(&a, &b);
        let dot_scalar = quantized_dot_product_scalar(&a, &b);

        assert_eq!(dot_simd, dot_scalar);
        // 1*8 + 2*7 + 3*6 + 4*5 + 5*4 + 6*3 + 7*2 + 8*1 = 8+14+18+20+20+18+14+8 = 120
        assert_eq!(dot_simd, 120);
    }

    #[test]
    fn test_quantized_dot_product_large() {
        // Test with larger vectors (768 dimensions)
        let a: Vec<u8> = (0..768).map(|i| (i % 256) as u8).collect();
        let b: Vec<u8> = (0..768).map(|i| ((255 - i) % 256) as u8).collect();

        let dot_simd = quantized_dot_product_simd(&a, &b);
        let dot_scalar = quantized_dot_product_scalar(&a, &b);

        assert_eq!(dot_simd, dot_scalar);
    }

    #[test]
    fn test_quantized_euclidean_squared() {
        // Test quantized Euclidean distance (squared)
        let a = vec![10u8, 20, 30, 40];
        let b = vec![13u8, 24, 27, 45];

        let dist_simd = quantized_euclidean_squared_simd(&a, &b);
        let dist_scalar = quantized_euclidean_squared_scalar(&a, &b);

        assert_eq!(dist_simd, dist_scalar);
        // (10-13)^2 + (20-24)^2 + (30-27)^2 + (40-45)^2 = 9 + 16 + 9 + 25 = 59
        assert_eq!(dist_simd, 59);
    }

    #[test]
    fn test_quantized_euclidean_squared_large() {
        // Test with larger vectors (768 dimensions)
        let a: Vec<u8> = (0..768).map(|i| (i % 256) as u8).collect();
        let b: Vec<u8> = (0..768).map(|i| ((i + 5) % 256) as u8).collect();

        let dist_simd = quantized_euclidean_squared_simd(&a, &b);
        let dist_scalar = quantized_euclidean_squared_scalar(&a, &b);

        assert_eq!(dist_simd, dist_scalar);
    }

    #[test]
    fn test_quantized_edge_cases() {
        // Test with identical vectors
        let a = vec![100u8; 100];
        let b = vec![100u8; 100];

        assert_eq!(quantized_manhattan_distance_simd(&a, &b), 0);
        assert_eq!(quantized_euclidean_squared_simd(&a, &b), 0);

        // Test with maximum difference
        let c = vec![0u8; 100];
        let d = vec![255u8; 100];

        assert_eq!(quantized_manhattan_distance_simd(&c, &d), 255 * 100);
        assert_eq!(quantized_euclidean_squared_simd(&c, &d), 255 * 255 * 100);
    }

    #[test]
    fn test_quantized_simd_correctness() {
        // Comprehensive correctness test with random-like values
        let a: Vec<u8> = (0..1024).map(|i| ((i * 17 + 42) % 256) as u8).collect();
        let b: Vec<u8> = (0..1024).map(|i| ((i * 23 + 99) % 256) as u8).collect();

        // All SIMD implementations should match scalar
        let manhattan_simd = quantized_manhattan_distance_simd(&a, &b);
        let manhattan_scalar = quantized_manhattan_distance_scalar(&a, &b);
        assert_eq!(manhattan_simd, manhattan_scalar);

        let dot_simd = quantized_dot_product_simd(&a, &b);
        let dot_scalar = quantized_dot_product_scalar(&a, &b);
        assert_eq!(dot_simd, dot_scalar);

        let euclidean_simd = quantized_euclidean_squared_simd(&a, &b);
        let euclidean_scalar = quantized_euclidean_squared_scalar(&a, &b);
        assert_eq!(euclidean_simd, euclidean_scalar);
    }

    #[test]
    fn test_normalize_vector_simd() {
        let mut vec = vec![3.0, 4.0, 0.0];
        normalize_vector_simd(&mut vec);

        // Expected: [3/5, 4/5, 0] = [0.6, 0.8, 0.0]
        assert!((vec[0] - 0.6).abs() < 1e-6);
        assert!((vec[1] - 0.8).abs() < 1e-6);
        assert!((vec[2] - 0.0).abs() < 1e-6);

        // Check that norm is 1.0
        let norm_squared: f32 = vec.iter().map(|x| x * x).sum();
        assert!((norm_squared - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_vector_simd_large() {
        // Test with large vector (768 dimensions)
        let mut vec: Vec<f32> = (0..768).map(|i| (i % 100) as f32).collect();
        normalize_vector_simd(&mut vec);

        // Check that norm is 1.0
        let norm_squared: f32 = vec.iter().map(|x| x * x).sum();
        assert!((norm_squared - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_normalize_vector_simd_zero() {
        // Test with zero vector (should not panic)
        let mut vec = vec![0.0, 0.0, 0.0];
        normalize_vector_simd(&mut vec);

        // Should remain zero
        assert_eq!(vec, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_scale_vector_simd() {
        let mut vec = vec![1.0, 2.0, 3.0, 4.0];
        scale_vector_simd(&mut vec, 2.0);

        assert_eq!(vec, vec![2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn test_scale_vector_simd_large() {
        // Test with large vector (1024 dimensions)
        let mut vec: Vec<f32> = (0..1024).map(|i| i as f32).collect();
        scale_vector_simd(&mut vec, 0.5);

        for (i, &value) in vec.iter().enumerate() {
            assert!((value - (i as f32 * 0.5)).abs() < 1e-5);
        }
    }
}
