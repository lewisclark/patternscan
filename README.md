# patternscan
Searches for a contiguous array of bytes determined by the given pattern. The pattern can include supported wildcard characters, as shown [below](#wildcards).

## Wildcards
- `?` matches any byte

## Example Patterns
- `fe 00 68 98` - matches only `fe 00 68 98`
- `8d 11 ? ? 8f` - could match `8d 11 9e ef 8f` or `8d 11 0 0 8f` etc

## License
This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details
