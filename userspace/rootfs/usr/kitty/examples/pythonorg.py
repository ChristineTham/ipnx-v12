# The example programs from python.org's front page, as written there:
#   python pythonorg.py
# 1. Fibonacci series
a, b = 0, 1
while a < 1000:
    print(a, end=',')
    a, b = b, a + b
print()
# 2. Compound data types
fruits = ['Banana', 'Apple', 'Lime']
loud_fruits = [fruit.upper() for fruit in fruits]
print(loud_fruits)
print(list(enumerate(fruits)))
# 3. Arithmetic
print(8 / 3)
# 4. Functions + control flow
def greet(name):
    print("Hello", name)
greet("Kitty")
