# patternscan
Searches for a contiguous array of bytes determined by a given pattern. The pattern can include supported wildcard characters, as shown [below](#wildcards).

## Wildcards
- `?` match any byte

## Example Patterns
- `fe 00 68 98` - matches only `fe 00 68 98`
- `8d 11 ? ? 8f` - could match `8d 11 9e ef 8f` or `8d 11 0 0 8f` for example

## License
This project is licensed under the MIT License - see [LICENSE.md](LICENSE.md) for details.
