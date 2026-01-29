#!/usr/bin/env python3
"""Generate YAML entries for cover.yaml from cover_POS.txt"""

def assign_weights(pos_tags):
    """Assign weights to POS tags that sum to 1.0"""
    pos_list = [tag.strip() for tag in pos_tags.split(',')]
    num_tags = len(pos_list)
    
    if num_tags == 1:
        return {pos_list[0]: 1.0}
    elif num_tags == 2:
        # Common pattern: first tag is usually more common
        return {pos_list[0]: 0.6, pos_list[1]: 0.4}
    elif num_tags == 3:
        return {pos_list[0]: 0.5, pos_list[1]: 0.3, pos_list[2]: 0.2}
    elif num_tags == 4:
        return {pos_list[0]: 0.4, pos_list[1]: 0.3, pos_list[2]: 0.2, pos_list[3]: 0.1}
    elif num_tags == 5:
        return {pos_list[0]: 0.35, pos_list[1]: 0.25, pos_list[2]: 0.2, pos_list[3]: 0.1, pos_list[4]: 0.1}
    else:
        # Equal distribution for 6+ tags
        weight = 1.0 / num_tags
        return {pos: weight for pos in pos_list}

def generate_yaml_entries(pos_file_path, start_line=10):
    """Generate YAML entries from POS file starting at specified line"""
    entries = []
    
    with open(pos_file_path, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    
    for i, line in enumerate(lines[start_line-1:], start=start_line):
        line = line.strip()
        if not line:
            continue
        
        parts = line.split('|')
        if len(parts) != 2:
            continue
        
        word = parts[0].strip()
        pos_tags = parts[1].strip()
        
        if not word or not pos_tags:
            continue
        
        weights = assign_weights(pos_tags)
        
        # Format YAML entry
        entry = f"{word}:"
        for pos, weight in sorted(weights.items()):
            entry += f"\n  {pos}: {weight}"
        entry += "\n"
        
        entries.append(entry)
    
    return entries

if __name__ == '__main__':
    import sys
    from pathlib import Path
    
    script_dir = Path(__file__).parent
    pos_file = script_dir / 'cover_POS.txt'
    output_file = script_dir / 'cover.yaml'
    
    entries = generate_yaml_entries(pos_file, start_line=10)
    
    # Append to existing cover.yaml
    with open(output_file, 'a', encoding='utf-8') as f:
        f.write('\n')
        f.write('\n'.join(entries))
    
    print(f"Appended {len(entries)} YAML entries to {output_file}")
