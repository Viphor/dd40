pub struct User {
    pub id: u64,
    pub name: String,
}

pub fn get_user(id: u64) -> Option<User> {
    // Placeholder implementation
    Some(User {
        id,
        name: format!("User{}", id),
    })
}
