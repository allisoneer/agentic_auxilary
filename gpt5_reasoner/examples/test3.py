import unittest
from test1 import add_numbers, multiply_numbers
from test2 import is_prime, get_primes_up_to

class TestMathFunctions(unittest.TestCase):
    def test_add_numbers(self):
        self.assertEqual(add_numbers(2, 3), 5)
        self.assertEqual(add_numbers(-1, 1), 0)

    def test_multiply_numbers(self):
        self.assertEqual(multiply_numbers(3, 4), 12)
        self.assertEqual(multiply_numbers(0, 5), 0)

    def test_is_prime(self):
        self.assertTrue(is_prime(2))
        self.assertTrue(is_prime(17))
        self.assertFalse(is_prime(1))
        self.assertFalse(is_prime(15))

    def test_get_primes_up_to(self):
        self.assertEqual(get_primes_up_to(10), [2, 3, 5, 7])

if __name__ == '__main__':
    unittest.main()