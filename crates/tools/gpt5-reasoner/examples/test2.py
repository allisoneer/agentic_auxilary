def is_prime(n):
    """Check if a number is prime."""
    if n < 2:
        return False
    for i in range(2, int(n**0.5) + 1):
        if n % i == 0:
            return False
    return True

def get_primes_up_to(limit):
    """Get all prime numbers up to a given limit."""
    return [n for n in range(2, limit + 1) if is_prime(n)]