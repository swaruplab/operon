import os

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

DATA_DIR = {
    'cell_type_data': os.path.join(PROJECT_ROOT, 'CellTypeAgent', 'data', 'GPTCellType'),
    'datasets': os.path.join(PROJECT_ROOT, 'CellTypeAgent', 'data', 'GPTCellType', 'datasets'),
    'logs': os.path.join(PROJECT_ROOT, 'CellTypeAgent', 'logs'),
    'analysis': os.path.join(PROJECT_ROOT, 'CellTypeAgent', 'analysis')
}

for path in DATA_DIR.values():
    os.makedirs(path, exist_ok=True)
