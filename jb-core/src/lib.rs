pub mod bili;
pub mod pixiv;
pub mod youtube;

pub fn encode_cookies<K, V>(cookies: impl IntoIterator<Item = (K, V)>) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut res = String::new();

    for (i, (k, v)) in cookies.into_iter().enumerate() {
        if i > 0 {
            res.push_str("; ");
        }
        res.push_str(k.as_ref());
        res.push('=');
        res.push_str(v.as_ref());
    }
    res
}
