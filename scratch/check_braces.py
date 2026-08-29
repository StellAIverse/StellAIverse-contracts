
import sys
import re

def check_braces(filename):
    with open(filename, 'r') as f:
        content = f.read()
    
    # Remove strings
    content = re.sub(r'"([^"\\]|\\.)*"', '""', content)
    # Remove single line comments
    content = re.sub(r'//.*', '', content)
    # Remove multi-line comments
    content = re.sub(r'/\*.*?\*/', '', content, flags=re.DOTALL)
    
    stack = []
    lines = content.split('\n')
    for i, line in enumerate(lines):
        for j, char in enumerate(line):
            if char == '{':
                stack.append((i+1, j+1))
            elif char == '}':
                if not stack:
                    print(f"Extra closing brace at line {i+1}, col {j+1}")
                else:
                    stack.pop()
    
    if stack:
        print(f"Unclosed opening braces: {len(stack)}")
        for line_num, col_num in stack:
            print(f"Unclosed brace at line {line_num}, col {col_num}")
    else:
        print("Braces are balanced (ignoring strings and comments)!")

if __name__ == "__main__":
    check_braces(sys.argv[1])
