import os
import json

def normalize_keys(input_dict):
    """ Recursively convert all dictionary keys to lowercase."""
    if isinstance(input_dict, dict):
        return {k.lower(): normalize_keys(v) for k, v in input_dict.items()}
    elif isinstance(input_dict, list):
        return [normalize_keys(item) for item in input_dict]
    else:
        return input_dict

def load_tool_links(tool_name, tools_dir):
    """ Reads Task_Knowledge.json and returns a list of sources under metadata """
    json_file_path = os.path.join(tools_dir, "Task_Knowledge.json")
    if os.path.exists(json_file_path):
        with open(json_file_path, "r", encoding="utf-8") as file:
            data = json.load(file)
        
        sources = []
        for item in data:
            if "metadata" in item and "source" in item["metadata"]:
                sources.append(item["metadata"]["source"])
        
        return sources
    return []
