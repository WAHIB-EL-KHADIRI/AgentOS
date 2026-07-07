pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Vec<f32>;
    fn dimensions(&self) -> usize;
}

#[derive(Debug, Clone, Copy)]
pub struct HashingEmbedder {
    dimensions: usize,
}

impl HashingEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

impl Default for HashingEmbedder {
    fn default() -> Self {
        Self { dimensions: 64 }
    }
}

impl Embedder for HashingEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let dims = self.dimensions.max(1);
        let mut vector = vec![0.0f32; dims];

        for token in text.split_whitespace() {
            let index = token.bytes().fold(0usize, |hash, byte| {
                hash.wrapping_mul(31).wrapping_add(byte as usize)
            }) % dims;
            vector[index] += 1.0;
        }

        let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for val in vector.iter_mut() {
                *val /= magnitude;
            }
        }

        vector
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hashing_embedder_produces_correct_dimensions() {
        let embedder = HashingEmbedder::new(128);
        let vec = embedder.embed("hello world");
        assert_eq!(vec.len(), 128);
    }

    #[test]
    fn test_hashing_embedder_normalized() {
        let embedder = HashingEmbedder::new(64);
        let vec = embedder.embed("test data for embedding");
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_hashing_embedder_deterministic() {
        let embedder = HashingEmbedder::new(32);
        let a = embedder.embed("same text");
        let b = embedder.embed("same text");
        assert_eq!(a, b);
    }

    #[test]
    fn test_similar_texts_have_similar_embeddings() {
        let embedder = HashingEmbedder::new(256);
        let a = embedder.embed("the cat sat on the mat");
        let b = embedder.embed("the cat sat on a mat");
        let c = embedder.embed("quantum physics is interesting");

        let similarity_ab: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let similarity_ac: f32 = a.iter().zip(c.iter()).map(|(x, y)| x * y).sum();

        assert!(
            similarity_ab > similarity_ac,
            "similar texts should have higher cosine similarity"
        );
    }
}
