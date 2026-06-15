import pandas as pd
import openpyxl

def load_data(reference_file, expr_file):
    wb = openpyxl.load_workbook(reference_file)
    sheet_names = wb.sheetnames
    expr_df = pd.read_csv(expr_file, header=0)
    expr_df.columns = expr_df.columns.str.strip()
    return sheet_names, expr_df
