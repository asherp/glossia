use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use serde_yaml::{Value, Mapping};

fn cleanup_weights(yaml_file: &PathBuf, threshold: f64) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(yaml_file)?;
    let mut data: Mapping = serde_yaml::from_str(&content)?;
    
    let mut changes_made = false;
    let mut words_removed = 0;
    
    for (_word_key, pos_weights_value) in data.iter_mut() {
        if let Some(pos_weights) = pos_weights_value.as_mapping_mut() {
            // Filter out weights below threshold
            let mut filtered: HashMap<String, f64> = HashMap::new();
            
            for (pos_key, weight_value) in pos_weights.iter() {
                if let (Some(pos_str), Some(weight)) = (pos_key.as_str(), weight_value.as_f64()) {
                    if weight >= threshold {
                        filtered.insert(pos_str.to_string(), weight);
                    } else {
                        changes_made = true;
                    }
                }
            }
            
            // If all weights were removed, keep the highest weight instead
            if filtered.is_empty() && !pos_weights.is_empty() {
                let mut max_weight = 0.0;
                let mut max_pos = None;
                for (pos_key, weight_value) in pos_weights.iter() {
                    if let (Some(pos_str), Some(weight)) = (pos_key.as_str(), weight_value.as_f64()) {
                        if weight > max_weight {
                            max_weight = weight;
                            max_pos = Some(pos_str.to_string());
                        }
                    }
                }
                if let Some(pos) = max_pos {
                    filtered.insert(pos, max_weight);
                    words_removed += 1;
                }
            }
            
            // Renormalize weights to sum to 1.0
            let total: f64 = filtered.values().sum();
            if total > 0.0 {
                let mut normalized = Mapping::new();
                for (pos, weight) in filtered.iter() {
                    let normalized_weight = weight / total;
                    normalized.insert(
                        Value::String(pos.clone()),
                        Value::Number(serde_yaml::Number::from(normalized_weight))
                    );
                }
                *pos_weights = normalized;
            }
        }
    }
    
    if changes_made || words_removed > 0 {
        // Sort keys alphabetically
        let mut sorted_data = Mapping::new();
        let mut keys: Vec<_> = data.keys().collect();
        keys.sort_by(|a, b| {
            let a_str = a.as_str().unwrap_or("");
            let b_str = b.as_str().unwrap_or("");
            a_str.cmp(b_str)
        });
        
        for key in keys {
            if let Some(value) = data.get(key) {
                sorted_data.insert(key.clone(), value.clone());
            }
        }
        
        let output = serde_yaml::to_string(&sorted_data)?;
        fs::write(yaml_file, output)?;
        println!("Cleaned up {:?}: removed low-weight POS tags, {} words had all weights below threshold", 
                 yaml_file, words_removed);
        Ok(())
    } else {
        println!("No changes needed for {:?}", yaml_file);
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cleanup_weights <yaml_file> [threshold]");
        std::process::exit(1);
    }
    
    let yaml_file = PathBuf::from(&args[1]);
    let threshold = if args.len() > 2 {
        args[2].parse::<f64>()?
    } else {
        0.1
    };
    
    if !yaml_file.exists() {
        eprintln!("Error: {:?} does not exist", yaml_file);
        std::process::exit(1);
    }
    
    cleanup_weights(&yaml_file, threshold)
}
